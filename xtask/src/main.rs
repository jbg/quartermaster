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
    let backend_team = required_env("QM_IOS_TEAM_ID")?;
    let backend_bundle = required_env("QM_IOS_BUNDLE_ID")?;
    let ios_team = required_env("QUARTERMASTER_IOS_DEVELOPMENT_TEAM")?;
    let ios_bundle = required_env("QUARTERMASTER_IOS_BUNDLE_ID")?;
    let associated_domain = required_env("QUARTERMASTER_ASSOCIATED_DOMAIN")?;

    validate_bare_hostname(&associated_domain, "QUARTERMASTER_ASSOCIATED_DOMAIN")?;

    let backend_identity = qm_api::IosReleaseIdentity::new(backend_team, backend_bundle)
        .map_err(anyhow::Error::msg)?;
    let ios_identity =
        qm_api::IosReleaseIdentity::new(ios_team, ios_bundle).map_err(anyhow::Error::msg)?;

    if backend_identity != ios_identity {
        bail!(
            "AASA app ID drift: backend serves {}, but iOS release identity implies {}",
            backend_identity.app_id(),
            ios_identity.app_id()
        );
    }

    if let Some(public_base_url) = env::var_os("QM_PUBLIC_BASE_URL") {
        let url = reqwest::Url::parse(
            &public_base_url
                .into_string()
                .map_err(|_| anyhow::anyhow!("QM_PUBLIC_BASE_URL must be valid UTF-8"))?,
        )
        .context("parsing QM_PUBLIC_BASE_URL")?;
        let public_host = url
            .host_str()
            .context("QM_PUBLIC_BASE_URL must be an origin URL")?;
        if public_host != associated_domain {
            bail!(
                "QM_PUBLIC_BASE_URL host {public_host} does not match QUARTERMASTER_ASSOCIATED_DOMAIN {associated_domain}"
            );
        }
    }

    println!(
        "verified release config: app_id={} domain={associated_domain}",
        backend_identity.app_id()
    );
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

fn required_env(name: &str) -> anyhow::Result<String> {
    let value = env::var(name).with_context(|| format!("missing required env var {name}"))?;
    if value.trim().is_empty() {
        bail!("{name} must not be blank");
    }
    Ok(value)
}

fn validate_bare_hostname(value: &str, env_name: &str) -> anyhow::Result<()> {
    if value.contains("://")
        || value.contains('/')
        || value.contains('?')
        || value.contains('#')
        || value.contains(':')
        || value.contains(' ')
    {
        bail!("{env_name} must be a bare hostname");
    }
    if !value
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || ch == '.' || ch == '-')
    {
        bail!("{env_name} must be a bare hostname");
    }
    Ok(())
}

fn print_help() {
    println!("usage: cargo xtask <subcommand>");
    println!();
    println!("subcommands:");
    println!("  export-openapi          write openapi.json to the repo root");
    println!("  seed-ledger-fixture     seed a small DB fixture for ledger verification");
    println!(
        "  verify-release-config   assert env-driven backend and iOS release identity stay aligned"
    );
    println!("  verify-stock-ledger     assert cached quantities match the event log");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_bare_hostname() {
        validate_bare_hostname(
            "quartermaster.example.com",
            "QUARTERMASTER_ASSOCIATED_DOMAIN",
        )
        .unwrap();
    }

    #[test]
    fn rejects_non_hostname_domain() {
        let err =
            validate_bare_hostname("https://quartermaster.example.com", "DOMAIN").unwrap_err();
        assert!(err.to_string().contains("bare hostname"));
    }
}
