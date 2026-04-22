//! Database layer for Quartermaster.
//!
//! The pool is `sqlx::Any` so one connection string can target either SQLite
//! (default, single-file self-host) or Postgres (optional). Repos live in
//! per-entity modules and take `&Database` as their first argument.

use std::str::FromStr;

use sqlx::any::AnyPoolOptions;
use sqlx::AnyPool;
#[cfg(any(test, feature = "test-support"))]
use std::sync::Arc;

pub mod auth_sessions;
pub mod barcode_cache;
pub mod devices;
pub mod households;
pub mod invites;
pub mod locations;
pub mod memberships;
pub mod products;
pub mod reminders;
pub mod stock;
pub mod stock_events;
#[cfg(any(test, feature = "test-support"))]
pub mod test_support;
pub mod time;
pub mod tokens;
pub mod users;

#[derive(Clone, Debug)]
pub struct Database {
    pub pool: AnyPool,
    backend: Backend,
    #[cfg(any(test, feature = "test-support"))]
    test_hooks: Arc<test_support::TestHooks>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Backend {
    Sqlite,
    Postgres,
    Other,
}

impl Database {
    /// Connect to the database given a URL like `sqlite://data.db?mode=rwc` or
    /// `postgres://user:pass@host/db`. Installs the default `Any` drivers on
    /// first call (safe to call repeatedly).
    pub async fn connect(url: &str) -> Result<Self, sqlx::Error> {
        sqlx::any::install_default_drivers();

        let backend = backend_from_url(url);
        let opts = sqlx::any::AnyConnectOptions::from_str(url)?;
        let pool = AnyPoolOptions::new()
            .after_connect(move |conn, _meta| {
                Box::pin(async move {
                    if backend == Backend::Sqlite {
                        // SQLite does not enforce foreign keys unless
                        // explicitly enabled on each connection.
                        sqlx::query("PRAGMA foreign_keys = ON")
                            .execute(&mut *conn)
                            .await?;
                        // Let concurrent writers wait briefly instead of
                        // surfacing immediate "database is locked" errors.
                        sqlx::query("PRAGMA busy_timeout = 5000")
                            .execute(&mut *conn)
                            .await?;
                    }
                    Ok(())
                })
            })
            .max_connections(8)
            .connect_with(opts)
            .await?;

        Ok(Self {
            pool,
            backend,
            #[cfg(any(test, feature = "test-support"))]
            test_hooks: Arc::new(test_support::TestHooks::default()),
        })
    }

    pub async fn migrate(&self) -> Result<(), sqlx::migrate::MigrateError> {
        sqlx::migrate!("./migrations").run(&self.pool).await
    }

    pub fn backend(&self) -> Backend {
        self.backend
    }
}

pub fn now_utc_rfc3339() -> String {
    time::now_utc_rfc3339()
}

#[cfg(any(test, feature = "test-support"))]
impl Database {
    pub async fn install_invite_race_gate(&self, gate: Arc<test_support::InviteRaceGate>) {
        self.test_hooks.install_invite_race_gate(gate).await;
    }

    pub async fn clear_invite_race_gate(&self) {
        self.test_hooks.clear_invite_race_gate().await;
    }

    pub(crate) async fn invite_race_gate(&self) -> Option<Arc<test_support::InviteRaceGate>> {
        self.test_hooks.invite_race_gate().await
    }

    pub async fn install_reminder_delivery_race_gate(
        &self,
        gate: Arc<test_support::ReminderDeliveryRaceGate>,
    ) {
        self.test_hooks
            .install_reminder_delivery_race_gate(gate)
            .await;
    }

    pub async fn clear_reminder_delivery_race_gate(&self) {
        self.test_hooks.clear_reminder_delivery_race_gate().await;
    }

    pub(crate) async fn reminder_delivery_race_gate(
        &self,
    ) -> Option<Arc<test_support::ReminderDeliveryRaceGate>> {
        self.test_hooks.reminder_delivery_race_gate().await
    }
}

fn backend_from_url(url: &str) -> Backend {
    if url.starts_with("sqlite") {
        Backend::Sqlite
    } else if url.starts_with("postgres") || url.starts_with("postgresql") {
        Backend::Postgres
    } else {
        Backend::Other
    }
}

#[cfg(test)]
async fn test_db() -> Database {
    test_support::sqlite().await.into_db()
}
