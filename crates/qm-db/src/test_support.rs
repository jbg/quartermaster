use std::{any::Any, str::FromStr};

use sqlx::{
    any::AnyPoolOptions,
    postgres::PgConnection,
    Connection, Executor,
};
use testcontainers::ContainerAsync;
use testcontainers_modules::{postgres::Postgres, testcontainers::runners::AsyncRunner};
use uuid::Uuid;

use crate::{Backend, Database};

const ENV_POSTGRES_URL: &str = "QM_POSTGRES_TEST_URL";
const ENV_RUN_POSTGRES: &str = "QM_RUN_POSTGRES_TESTS";
const ENV_REQUIRE_POSTGRES: &str = "QM_REQUIRE_POSTGRES_TESTS";

pub struct TestDatabase {
    db: Database,
    _guard: Option<Box<dyn Any + Send>>,
}

impl TestDatabase {
    pub fn db(&self) -> &Database {
        &self.db
    }

    pub fn into_db(self) -> Database {
        self.db
    }

    pub fn backend(&self) -> Backend {
        self.db.backend()
    }
}

pub async fn sqlite() -> TestDatabase {
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
    sqlx::query("PRAGMA busy_timeout = 5000")
        .execute(&pool)
        .await
        .expect("busy_timeout");
    let db = Database {
        pool,
        backend: Backend::Sqlite,
    };
    db.migrate().await.expect("migrate");
    TestDatabase { db, _guard: None }
}

pub async fn postgres() -> Option<TestDatabase> {
    let require = env_truthy(ENV_REQUIRE_POSTGRES);
    match postgres_inner().await {
        Ok(db) => db,
        Err(err) if require => panic!("postgres test database required: {err:#}"),
        Err(err) => {
            eprintln!("skipping Postgres tests: {err:#}");
            None
        }
    }
}

async fn postgres_inner() -> anyhow::Result<Option<TestDatabase>> {
    let admin_url = if let Ok(url) = std::env::var(ENV_POSTGRES_URL) {
        url
    } else if env_truthy(ENV_RUN_POSTGRES) {
        let container = Postgres::default().start().await?;
        let host = container.get_host().await?;
        let port = container.get_host_port_ipv4(5432).await?;
        let url = format!("postgres://postgres:postgres@{host}:{port}/postgres");
        return connect_isolated_postgres(url, Some(Box::new(container))).await.map(Some);
    } else {
        return Ok(None);
    };

    connect_isolated_postgres(admin_url, None).await.map(Some)
}

async fn connect_isolated_postgres(
    admin_url: String,
    guard: Option<Box<dyn Any + Send>>,
) -> anyhow::Result<TestDatabase> {
    sqlx::any::install_default_drivers();

    let db_name = format!("qm_test_{}", Uuid::now_v7().simple());
    let mut admin = PgConnection::connect(&admin_url).await?;
    admin
        .execute(format!(r#"CREATE DATABASE "{db_name}""#).as_str())
        .await?;
    admin.close().await?;

    let db_url = db_url_with_database(&admin_url, &db_name);
    let db = Database::connect(&db_url).await?;
    db.migrate().await?;

    Ok(TestDatabase { db, _guard: guard })
}

fn db_url_with_database(admin_url: &str, db_name: &str) -> String {
    let (head, query) = admin_url.split_once('?').map_or((admin_url, ""), |(h, q)| (h, q));
    let (prefix, _) = head.rsplit_once('/').expect("postgres url should include database");
    if query.is_empty() {
        format!("{prefix}/{db_name}")
    } else {
        format!("{prefix}/{db_name}?{query}")
    }
}

fn env_truthy(name: &str) -> bool {
    matches!(
        std::env::var(name).ok().as_deref(),
        Some("1") | Some("true") | Some("TRUE") | Some("yes") | Some("YES")
    )
}

pub fn postgres_test_enabled() -> bool {
    std::env::var(ENV_POSTGRES_URL).is_ok() || env_truthy(ENV_RUN_POSTGRES)
}

pub type PostgresContainer = ContainerAsync<Postgres>;
