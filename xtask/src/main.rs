use std::{env, path::PathBuf, process::ExitCode, str::FromStr};

use anyhow::{bail, Context};
use rust_decimal::Decimal;
use sqlx::Row;
use utoipa::OpenApi;

fn main() -> ExitCode {
    let cmd = env::args().nth(1).unwrap_or_default();
    let result = match cmd.as_str() {
        "export-openapi" => export_openapi(),
        "verify-release-config" => verify_release_config(),
        "verify-stock-ledger" => tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("runtime")
            .block_on(verify_stock_ledger()),
        "seed-ledger-fixture" => tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("runtime")
            .block_on(seed_ledger_fixture()),
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

    // Write to two places: the canonical copy at the repo root (external
    // consumers, CI drift check) and the one the swift-openapi-generator
    // plugin actually reads when Xcode builds the iOS app.
    let root = repo_root()?;
    for out in [
        root.join("openapi.json"),
        root.join("ios/Quartermaster/openapi.json"),
    ] {
        std::fs::write(&out, &json).with_context(|| format!("writing {}", out.display()))?;
        println!("wrote {}", out.display());
    }
    Ok(())
}

fn verify_release_config() -> anyhow::Result<()> {
    let root = repo_root()?;
    let project_yml =
        std::fs::read_to_string(root.join("ios/project.yml")).context("reading ios/project.yml")?;
    let development_team = read_project_setting(&project_yml, "DEVELOPMENT_TEAM")
        .context("reading DEVELOPMENT_TEAM from ios/project.yml")?;
    let product_bundle_identifier = read_project_setting(&project_yml, "PRODUCT_BUNDLE_IDENTIFIER")
        .context("reading PRODUCT_BUNDLE_IDENTIFIER from ios/project.yml")?;

    let expected_app_id = format!("{development_team}.{product_bundle_identifier}");
    let actual_app_id = qm_api::routes::join::apple_app_site_association_app_id();

    if expected_app_id != actual_app_id {
        bail!(
            "AASA app ID drift: backend serves {actual_app_id}, but ios/project.yml implies {expected_app_id}"
        );
    }

    println!("verified release config: {actual_app_id}");
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
        let cached = Decimal::from_str(&cached_str).with_context(|| {
            format!("parsing cached quantity '{cached_str}' for batch {batch_id}")
        })?;

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

async fn seed_ledger_fixture() -> anyhow::Result<()> {
    let url = env::var("QM_DATABASE_URL").unwrap_or_else(|_| "sqlite://data.db?mode=rwc".into());
    println!("seeding stock ledger fixture into {url}");
    let db = qm_db::Database::connect(&url)
        .await
        .context("connecting to database")?;
    db.migrate().await.context("running migrations")?;

    let household = qm_db::households::create(&db, "Fixture Household", "UTC")
        .await
        .context("creating household")?;
    qm_db::locations::seed_defaults(&db, household.id)
        .await
        .context("seeding locations")?;
    let pantry = qm_db::locations::list_for_household(&db, household.id)
        .await
        .context("listing locations")?
        .into_iter()
        .find(|loc| loc.kind == "pantry")
        .context("finding pantry location")?;
    let user = qm_db::users::create(&db, "fixture-admin", Some("fixture@example.com"), "hash")
        .await
        .context("creating user")?;
    qm_db::memberships::insert(&db, household.id, user.id, "admin")
        .await
        .context("creating membership")?;
    let product = qm_db::products::create_manual(
        &db,
        household.id,
        "Fixture Rice",
        Some("Acme"),
        "mass",
        Some("g"),
        None,
        None,
    )
    .await
    .context("creating product")?;

    let batch = qm_db::stock::create(
        &db,
        household.id,
        product.id,
        pantry.id,
        "500",
        "g",
        Some("2026-12-31"),
        None,
        Some("fixture batch"),
        user.id,
        None,
    )
    .await
    .context("creating stock")?;
    qm_db::stock::adjust(
        &db,
        household.id,
        batch.id,
        "450",
        user.id,
        Some("fixture adjust"),
        None,
    )
    .await
    .context("adjusting stock")?;

    println!(
        "seeded fixture household={} batch={}",
        household.id, batch.id
    );
    Ok(())
}

async fn sum_ledger_for_batch(db: &qm_db::Database, batch_id: &str) -> anyhow::Result<Decimal> {
    let rows = sqlx::query("SELECT quantity_delta FROM stock_event WHERE batch_id = ?")
        .bind(batch_id)
        .fetch_all(&db.pool)
        .await
        .context("reading event log")?;
    let mut sum = Decimal::ZERO;
    for row in rows {
        let s: String = row.try_get("quantity_delta")?;
        let d = Decimal::from_str(&s).with_context(|| format!("parsing event delta '{s}'"))?;
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

fn read_project_setting(contents: &str, key: &str) -> anyhow::Result<String> {
    let prefix = format!("{key}:");
    let value = contents
        .lines()
        .map(str::trim)
        .find_map(|line| line.strip_prefix(&prefix).map(str::trim))
        .context(format!("missing {key}"))?;

    Ok(value.trim_matches('"').to_owned())
}

fn print_help() {
    println!("usage: cargo xtask <subcommand>");
    println!();
    println!("subcommands:");
    println!("  export-openapi          write openapi.json to the repo root");
    println!("  seed-ledger-fixture     seed a small DB fixture for ledger verification");
    println!("  verify-release-config   assert backend AASA app ID matches ios/project.yml");
    println!("  verify-stock-ledger     assert cached quantities match the event log");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_project_settings_from_project_yml() {
        let contents = r#"
settings:
  base:
    DEVELOPMENT_TEAM: "42J2SSX5SM"
targets:
  Quartermaster:
    settings:
      base:
        PRODUCT_BUNDLE_IDENTIFIER: com.jasperhugo.quartermaster
"#;
        assert_eq!(
            read_project_setting(contents, "DEVELOPMENT_TEAM").unwrap(),
            "42J2SSX5SM"
        );
        assert_eq!(
            read_project_setting(contents, "PRODUCT_BUNDLE_IDENTIFIER").unwrap(),
            "com.jasperhugo.quartermaster"
        );
    }
}
