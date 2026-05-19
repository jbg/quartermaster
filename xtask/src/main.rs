use std::{env, path::PathBuf, process::ExitCode, str::FromStr};

use anyhow::{bail, Context};
use rust_decimal::Decimal;
use sqlx::Row;

fn main() -> ExitCode {
    let cmd = env::args().nth(1).unwrap_or_default();
    let result = match cmd.as_str() {
        "configure-release-identity" => configure_release_identity(env::args().skip(2)),
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
    let spec = qm_api::openapi_spec();
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

fn configure_release_identity(args: impl IntoIterator<Item = String>) -> anyhow::Result<()> {
    let config = ReleaseIdentityArgs::parse(args)?;
    validate_release_identity_fields(
        &config.team,
        &config.bundle,
        &config.domain,
        "team",
        "bundle-id",
        "domain",
    )?;

    let identity = qm_api::IosReleaseIdentity::new(config.team.clone(), config.bundle.clone())
        .map_err(anyhow::Error::msg)?;
    let root = repo_root()?;
    let out = root.join("ios/Config/ReleaseIdentity.generated.xcconfig");
    let code_sign_style = if config.profile.is_empty() {
        "Automatic"
    } else {
        "Manual"
    };
    let contents = format!(
        "QUARTERMASTER_RELEASE_DEVELOPMENT_TEAM = {}\n\
         QUARTERMASTER_RELEASE_PRODUCT_BUNDLE_IDENTIFIER = {}\n\
         QUARTERMASTER_RELEASE_CODE_SIGN_STYLE = {}\n\
         QUARTERMASTER_RELEASE_CODE_SIGN_IDENTITY = {}\n\
         QUARTERMASTER_RELEASE_PROVISIONING_PROFILE_SPECIFIER = {}\n\
         QUARTERMASTER_ASSOCIATED_DOMAIN = {}\n",
        config.team,
        config.bundle,
        code_sign_style,
        config.signing_certificate,
        config.profile,
        config.domain
    );
    std::fs::create_dir_all(out.parent().context("locating iOS config dir")?)
        .with_context(|| format!("creating {}", out.parent().unwrap().display()))?;
    std::fs::write(&out, contents).with_context(|| format!("writing {}", out.display()))?;

    println!("wrote {}", out.display());
    println!(
        "verified release config: app_id={} domain={}",
        identity.app_id(),
        config.domain
    );
    println!("server environment:");
    println!("  QM_IOS_TEAM_ID={}", config.team);
    println!("  QM_IOS_BUNDLE_ID={}", config.bundle);
    println!("iOS environment:");
    println!("  QUARTERMASTER_IOS_DEVELOPMENT_TEAM={}", config.team);
    println!("  QUARTERMASTER_IOS_BUNDLE_ID={}", config.bundle);
    println!("  QUARTERMASTER_ASSOCIATED_DOMAIN={}", config.domain);
    Ok(())
}

#[derive(Debug, PartialEq, Eq)]
struct ReleaseIdentityArgs {
    team: String,
    bundle: String,
    domain: String,
    profile: String,
    signing_certificate: String,
}

impl ReleaseIdentityArgs {
    fn parse(args: impl IntoIterator<Item = String>) -> anyhow::Result<Self> {
        let mut team = None;
        let mut bundle = None;
        let mut domain = None;
        let mut profile = String::new();
        let mut signing_certificate = "Apple Distribution".to_string();
        let mut iter = args.into_iter();
        while let Some(arg) = iter.next() {
            match arg.as_str() {
                "--team" | "--team-id" => team = Some(required_arg_value(&arg, &mut iter)?),
                "--bundle" | "--bundle-id" => bundle = Some(required_arg_value(&arg, &mut iter)?),
                "--domain" | "--associated-domain" => {
                    domain = Some(required_arg_value(&arg, &mut iter)?)
                }
                "--profile" => profile = required_arg_value(&arg, &mut iter)?,
                "--signing-certificate" => {
                    signing_certificate = required_arg_value(&arg, &mut iter)?
                }
                "--help" | "-h" => bail!(release_identity_usage()),
                other => bail!("unknown configure-release-identity option: {other}"),
            }
        }
        Ok(Self {
            team: required_option(team, "--team")?,
            bundle: required_option(bundle, "--bundle-id")?,
            domain: required_option(domain, "--domain")?,
            profile,
            signing_certificate,
        })
    }
}

fn required_arg_value(
    option: &str,
    iter: &mut impl Iterator<Item = String>,
) -> anyhow::Result<String> {
    let value = iter
        .next()
        .with_context(|| format!("{option} requires a value"))?;
    if value.trim().is_empty() || value.starts_with("--") {
        bail!("{option} requires a value");
    }
    Ok(value)
}

fn required_option(value: Option<String>, option: &str) -> anyhow::Result<String> {
    let value = value.with_context(|| format!("missing required option {option}"))?;
    if value.trim().is_empty() {
        bail!("{option} must not be blank");
    }
    Ok(value)
}

fn release_identity_usage() -> &'static str {
    "usage: cargo xtask configure-release-identity --team TEAM_ID --bundle-id BUNDLE_ID --domain HOSTNAME [--profile PROFILE] [--signing-certificate NAME]"
}

fn verify_release_config() -> anyhow::Result<()> {
    let backend_team = required_env("QM_IOS_TEAM_ID")?;
    let backend_bundle = required_env("QM_IOS_BUNDLE_ID")?;
    let ios_team = required_env("QUARTERMASTER_IOS_DEVELOPMENT_TEAM")?;
    let ios_bundle = required_env("QUARTERMASTER_IOS_BUNDLE_ID")?;
    let associated_domain = required_env("QUARTERMASTER_ASSOCIATED_DOMAIN")?;

    validate_release_identity_fields(
        &backend_team,
        &backend_bundle,
        &associated_domain,
        "QM_IOS_TEAM_ID",
        "QM_IOS_BUNDLE_ID",
        "QUARTERMASTER_ASSOCIATED_DOMAIN",
    )?;
    validate_release_identity_fields(
        &ios_team,
        &ios_bundle,
        &associated_domain,
        "QUARTERMASTER_IOS_DEVELOPMENT_TEAM",
        "QUARTERMASTER_IOS_BUNDLE_ID",
        "QUARTERMASTER_ASSOCIATED_DOMAIN",
    )?;

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
    let user = qm_db::users::create(&db, "fixture@example.com", "Fixture Admin", "hash")
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
        None,
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

fn validate_release_identity_fields(
    team: &str,
    bundle: &str,
    domain: &str,
    team_name: &str,
    bundle_name: &str,
    domain_name: &str,
) -> anyhow::Result<()> {
    if !team.chars().all(|ch| ch.is_ascii_alphanumeric()) {
        bail!("{team_name} must be ASCII alphanumeric");
    }
    if !bundle
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || ch == '.' || ch == '-')
    {
        bail!("{bundle_name} must contain only ASCII alphanumeric characters, dots, or hyphens");
    }
    validate_bare_hostname(domain, domain_name)?;
    Ok(())
}

fn print_help() {
    println!("usage: cargo xtask <subcommand>");
    println!();
    println!("subcommands:");
    println!("  configure-release-identity  generate iOS release config and matching server env");
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

    #[test]
    fn parses_release_identity_args() {
        let args = ReleaseIdentityArgs::parse(
            [
                "--team",
                "ABC123",
                "--bundle-id",
                "dev.quartermaster.app",
                "--domain",
                "quartermaster.example.com",
            ]
            .into_iter()
            .map(String::from),
        )
        .unwrap();
        assert_eq!(
            args,
            ReleaseIdentityArgs {
                team: "ABC123".into(),
                bundle: "dev.quartermaster.app".into(),
                domain: "quartermaster.example.com".into(),
                profile: String::new(),
                signing_certificate: "Apple Distribution".into(),
            }
        );
    }

    #[test]
    fn rejects_missing_release_identity_arg() {
        let err = ReleaseIdentityArgs::parse(
            ["--team", "ABC123", "--domain", "quartermaster.example.com"]
                .into_iter()
                .map(String::from),
        )
        .unwrap_err();
        assert!(err
            .to_string()
            .contains("missing required option --bundle-id"));
    }

    #[test]
    fn validates_release_identity_fields() {
        validate_release_identity_fields(
            "ABC123",
            "dev.quartermaster.app",
            "quartermaster.example.com",
            "team",
            "bundle",
            "domain",
        )
        .unwrap();

        let err = validate_release_identity_fields(
            "ABC-123",
            "dev.quartermaster.app",
            "quartermaster.example.com",
            "team",
            "bundle",
            "domain",
        )
        .unwrap_err();
        assert!(err.to_string().contains("ASCII alphanumeric"));
    }
}
