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

pub mod ai_tasks;
pub mod auth_handoff;
pub mod auth_sessions;
pub mod barcode_cache;
pub mod billing;
pub mod devices;
pub mod household_exports;
pub mod households;
pub mod ingredients;
pub mod invites;
pub mod jobs;
pub mod label_printers;
pub mod locations;
pub mod meal_plans;
pub mod memberships;
pub mod off_credentials;
pub mod pantry_suggestions;
pub mod passkeys;
pub mod products;
pub mod quotas;
pub mod recipes;
pub mod reminders;
pub mod replenishment;
pub mod stock;
pub mod stock_events;
pub mod storage_vessels;
pub mod suppliers;
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
        sqlx::migrate!("./migrations").run(&self.pool).await?;
        self.drop_legacy_product_off_barcode_unique()
            .await
            .map_err(sqlx::migrate::MigrateError::Execute)?;
        Ok(())
    }

    pub fn backend(&self) -> Backend {
        self.backend
    }

    async fn drop_legacy_product_off_barcode_unique(&self) -> Result<(), sqlx::Error> {
        match self.backend {
            Backend::Postgres => {
                sqlx::query(
                    "ALTER TABLE product DROP CONSTRAINT IF EXISTS product_off_barcode_key",
                )
                .execute(&self.pool)
                .await?;
            }
            Backend::Sqlite => {
                let row: Option<(String,)> = sqlx::query_as(
                    "SELECT sql FROM sqlite_master WHERE type = 'table' AND name = 'product'",
                )
                .fetch_optional(&self.pool)
                .await?;
                let Some((create_sql,)) = row else {
                    return Ok(());
                };
                if !create_sql.contains("off_barcode              TEXT UNIQUE")
                    && !create_sql.contains("off_barcode TEXT UNIQUE")
                {
                    return Ok(());
                }

                // SQLite cannot drop a table-level UNIQUE constraint in place.
                // Rebuild the table once, preserving the later household-scoped
                // barcode index and keeping child FKs pointed at `product`.
                let mut conn = self.pool.acquire().await?;
                sqlx::query("PRAGMA foreign_keys = OFF")
                    .execute(&mut *conn)
                    .await?;
                sqlx::query("PRAGMA legacy_alter_table = ON")
                    .execute(&mut *conn)
                    .await?;
                sqlx::query("ALTER TABLE product RENAME TO product_legacy_unique")
                    .execute(&mut *conn)
                    .await?;
                sqlx::query(
                    "CREATE TABLE product ( \
                        id TEXT PRIMARY KEY, \
                        source TEXT NOT NULL, \
                        off_barcode TEXT, \
                        name TEXT NOT NULL, \
                        brand TEXT, \
                        default_unit TEXT NOT NULL, \
                        image_url TEXT, \
                        fetched_at TEXT, \
                        created_by_household_id TEXT REFERENCES household(id) ON DELETE CASCADE, \
                        created_at TEXT NOT NULL, \
                        family TEXT NOT NULL DEFAULT 'count', \
                        deleted_at TEXT, \
                        package_quantity TEXT, \
                        package_unit TEXT, \
                        max_open_days INTEGER, \
                        package_size_local_override INTEGER NOT NULL DEFAULT 0, \
                        off_name TEXT, \
                        off_brand TEXT, \
                        off_package_quantity TEXT, \
                        off_package_unit TEXT, \
                        name_local_override INTEGER NOT NULL DEFAULT 0, \
                        brand_local_override INTEGER NOT NULL DEFAULT 0, \
                        family_local_override INTEGER NOT NULL DEFAULT 0 \
                    )",
                )
                .execute(&mut *conn)
                .await?;
                sqlx::query(
                    "INSERT INTO product ( \
                        id, source, off_barcode, name, brand, default_unit, image_url, fetched_at, \
                        created_by_household_id, created_at, family, deleted_at, package_quantity, package_unit, \
                        max_open_days, package_size_local_override, off_name, off_brand, \
                        off_package_quantity, off_package_unit, name_local_override, \
                        brand_local_override, family_local_override \
                     ) \
                     SELECT \
                        id, source, off_barcode, name, brand, default_unit, image_url, fetched_at, \
                        created_by_household_id, created_at, family, deleted_at, package_quantity, package_unit, \
                        max_open_days, package_size_local_override, off_name, off_brand, \
                        off_package_quantity, off_package_unit, name_local_override, \
                        brand_local_override, family_local_override \
                     FROM product_legacy_unique",
                )
                .execute(&mut *conn)
                .await?;
                sqlx::query("DROP TABLE product_legacy_unique")
                    .execute(&mut *conn)
                    .await?;
                sqlx::query("PRAGMA legacy_alter_table = OFF")
                    .execute(&mut *conn)
                    .await?;
                sqlx::query("PRAGMA foreign_keys = ON")
                    .execute(&mut *conn)
                    .await?;
                sqlx::query("CREATE INDEX IF NOT EXISTS idx_product_household ON product(created_by_household_id)")
                    .execute(&mut *conn)
                    .await?;
                sqlx::query("CREATE INDEX IF NOT EXISTS idx_product_household_barcode ON product(created_by_household_id, off_barcode)")
                    .execute(&mut *conn)
                    .await?;
            }
            Backend::Other => {}
        }
        Ok(())
    }
}

pub fn now_utc_rfc3339() -> String {
    time::now_utc_rfc3339()
}

pub(crate) fn sql_for_backend(
    backend: Backend,
    sqlite_sql: &'static str,
    postgres_sql: &'static str,
) -> &'static str {
    match backend {
        Backend::Postgres => postgres_sql,
        Backend::Sqlite | Backend::Other => sqlite_sql,
    }
}

pub(crate) fn audited_sql(sql: String) -> sqlx::AssertSqlSafe<String> {
    sqlx::AssertSqlSafe(sql)
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
