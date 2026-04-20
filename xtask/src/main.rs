use std::{env, path::PathBuf};

use anyhow::{bail, Context};
use utoipa::OpenApi;

fn main() -> anyhow::Result<()> {
    let cmd = env::args().nth(1).unwrap_or_default();
    match cmd.as_str() {
        "export-openapi" => export_openapi(),
        "" | "help" | "--help" | "-h" => {
            print_help();
            Ok(())
        }
        other => {
            print_help();
            bail!("unknown subcommand: {other}");
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
    println!("  export-openapi    write openapi.json to the repo root");
}
