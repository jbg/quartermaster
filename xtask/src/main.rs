use std::{env, path::PathBuf, process::ExitCode, str::FromStr};

use anyhow::{bail, Context};
use rust_decimal::Decimal;
use sqlx::Row;
use utoipa::OpenApi;

fn main() -> ExitCode {
    let cmd = env::args().nth(1).unwrap_or_default();
    let result = match cmd.as_str() {
        "export-openapi" => export_openapi(),
        "verify-stock-ledger" => {
            tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("runtime")
                .block_on(verify_stock_ledger())
        }
        "" | "help" | "--help" | "-h" => {
            print_help();
            Ok(())
        }
        other => {
            print_help();
            Err(anyhow::anyhow!("unknown subcommand: {other}"))
        }
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("xtask: {err:#}");
            ExitCode::FAILURE
        }
    }
}

fn export_openapi() -> anyhow::Result<()> {
    let spec = qm_api::ApiDoc::openapi();
    let json = serde_json::to_string_pretty(&spec).context("serialising openapi spec")?;
    let out = repo_root()?.join("openapi.json");
    std::fs::write(&out, json).with_context(|| format!("writing {}", out.display()))?;
    println!("wrote {}", out.display());
    Ok(())
}

/// Walk every batch and assert its cached `quantity` equals the sum of its
/// event log. Prints one line per drifting batch; returns non-zero exit when
/// any drift is found.
///
/// Intended use: CI pre-flight against a seeded fixture DB, or a local
/// "did that migration land clean?" check after hand-editing the DB.
async fn verify_stock_ledger() -> anyhow::Result<()> {
    let url = env::var("QM_DATABASE_URL").unwrap_or_else(|_| "sqlite://data.db?mode=rwc".into());
    println!("verifying stock ledger against {url}");
    let db = qm_db::Database::connect(&url)
        .await
        .context("connecting to database")?;

    let rows = sqlx::query(
        "SELECT b.id AS batch_id, b.quantity AS cached, \
                COALESCE(( \
                    SELECT SUM(CAST(e.quantity_delta AS REAL)) FROM stock_event e WHERE e.batch_id = b.id \
                ), 0) AS ledger_numeric \
         FROM stock_batch b",
    )
    .fetch_all(&db.pool)
    .await
    .context("querying batches")?;

    // SQLite's SUM above returns REAL; we re-sum via rust_decimal for
    // precision instead of trusting the float output. The REAL is only used
    // as a cheap early-exit on batches that look fine.
    let mut drifted = 0usize;
    let mut total = 0usize;
    for row in rows {
        total += 1;
        let batch_id: String = row.try_get("batch_id")?;
        let cached_str: String = row.try_get("cached")?;
        let cached = Decimal::from_str(&cached_str)
            .with_context(|| format!("parsing cached quantity '{cached_str}' for batch {batch_id}"))?;

        let ledger = sum_ledger_for_batch(&db, &batch_id).await?;
        if cached != ledger {
            drifted += 1;
            println!("  DRIFT batch={batch_id} cached={cached} ledger_sum={ledger}");
        }
    }

    println!("{total} batches checked, {drifted} drifted");
    if drifted > 0 {
        bail!("ledger drift detected — rebuild cache or investigate");
    }
    Ok(())
}

async fn sum_ledger_for_batch(db: &qm_db::Database, batch_id: &str) -> anyhow::Result<Decimal> {
    let rows = sqlx::query(
        "SELECT quantity_delta FROM stock_event WHERE batch_id = ?",
    )
    .bind(batch_id)
    .fetch_all(&db.pool)
    .await
    .context("reading event log")?;
    let mut sum = Decimal::ZERO;
    for row in rows {
        let s: String = row.try_get("quantity_delta")?;
        let d = Decimal::from_str(&s)
            .with_context(|| format!("parsing event delta '{s}'"))?;
        sum += d;
    }
    Ok(sum)
}

fn repo_root() -> anyhow::Result<PathBuf> {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    PathBuf::from(manifest_dir)
        .parent()
        .map(PathBuf::from)
        .context("locating repo root")
}

fn print_help() {
    println!("usage: cargo xtask <subcommand>");
    println!();
    println!("subcommands:");
    println!("  export-openapi          write openapi.json to the repo root");
    println!("  verify-stock-ledger     assert cached quantities match the event log");
}
