//! Database layer for Quartermaster.
//!
//! The pool is `sqlx::Any` so one connection string can target either SQLite
//! (default, single-file self-host) or Postgres (optional). Repos live in
//! per-entity modules and take `&Database` as their first argument.

use std::str::FromStr;

use sqlx::any::AnyPoolOptions;
use sqlx::AnyPool;

pub mod barcode_cache;
pub mod households;
pub mod locations;
pub mod memberships;
pub mod products;
pub mod stock;
pub mod stock_events;
pub mod tokens;
pub mod users;

#[derive(Clone, Debug)]
pub struct Database {
    pub pool: AnyPool,
}

impl Database {
    /// Connect to the database given a URL like `sqlite://data.db?mode=rwc` or
    /// `postgres://user:pass@host/db`. Installs the default `Any` drivers on
    /// first call (safe to call repeatedly).
    pub async fn connect(url: &str) -> Result<Self, sqlx::Error> {
        sqlx::any::install_default_drivers();

        let opts = sqlx::any::AnyConnectOptions::from_str(url)?;
        let pool = AnyPoolOptions::new()
            .max_connections(8)
            .connect_with(opts)
            .await?;

        if url.starts_with("sqlite") {
            // SQLite does not enforce foreign keys unless explicitly asked.
            sqlx::query("PRAGMA foreign_keys = ON").execute(&pool).await?;
        }

        Ok(Self { pool })
    }

    pub async fn migrate(&self) -> Result<(), sqlx::migrate::MigrateError> {
        sqlx::migrate!("./migrations").run(&self.pool).await
    }
}

pub fn now_utc_rfc3339() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}

#[cfg(test)]
async fn test_db() -> Database {
    sqlx::any::install_default_drivers();
    // SQLite's private in-memory databases are per-connection, so force a
    // single connection in tests — otherwise migrations and queries land in
    // different databases.
    let opts = sqlx::any::AnyConnectOptions::from_str("sqlite::memory:").expect("opts");
    let pool = AnyPoolOptions::new()
        .max_connections(1)
        .connect_with(opts)
        .await
        .expect("connect");
    sqlx::query("PRAGMA foreign_keys = ON")
        .execute(&pool)
        .await
        .expect("foreign_keys");
    let db = Database { pool };
    db.migrate().await.expect("migrate");
    db
}
