use std::ops::Deref;
use std::sync::Arc;
#[cfg(not(feature = "postgres"))]
use sqlx::{sqlite::SqlitePoolOptions, SqlitePool};
#[cfg(feature = "postgres")]
use sqlx::{postgres::PgPoolOptions, PgPool};
use tracing::{error, info};

use crate::config::Config;

#[cfg(not(feature = "postgres"))]
pub type DbPool = SqlitePool;
#[cfg(feature = "postgres")]
pub type DbPool = PgPool;

/// Wraps writer and optional reader pools for read replica support.
///
/// When `DEKO_DATABASE_READ_URL` is set, use `.reader()` for read-only
/// SELECT queries and `.writer()` for INSERT/UPDATE/DELETE. Otherwise
/// both methods return the same pool.
///
/// The `DbPoolSet` is placed in request extensions so route handlers
/// can access it without changing their `State<DbPool>` signature.
#[derive(Clone)]
pub struct DbPoolSet {
    #[allow(dead_code)]
    writer: DbPool,
    #[allow(dead_code)]
    reader: DbPool,
}

impl DbPoolSet {
    pub fn new(writer: DbPool, reader: DbPool) -> Self {
        Self { writer, reader }
    }

    pub fn writer(&self) -> &DbPool {
        &self.writer
    }

    #[allow(dead_code)]
    pub fn reader(&self) -> &DbPool {
        &self.reader
    }
}

/// Auto-deref to the writer pool so &DbPoolSet can be used with sqlx.
impl Deref for DbPoolSet {
    type Target = DbPool;

    fn deref(&self) -> &Self::Target {
        self.writer()
    }
}

async fn create_pool(url: &str) -> anyhow::Result<DbPool> {
    #[cfg(not(feature = "postgres"))]
    let pool = SqlitePoolOptions::new()
        .max_connections(10)
        .acquire_timeout(std::time::Duration::from_secs(5))
        .connect(url)
        .await?;

    #[cfg(feature = "postgres")]
    let pool = PgPoolOptions::new()
        .max_connections(10)
        .acquire_timeout(std::time::Duration::from_secs(5))
        .connect(url)
        .await?;

    Ok(pool)
}

pub async fn init_db(config: &Config) -> anyhow::Result<(DbPool, Arc<DbPoolSet>)> {
    info!("Initializing database connection");

    let writer = create_pool(&config.database_url).await?;

    let reader = if let Some(ref reader_url) = config.database_read_url {
        info!("Using read replica: {}", reader_url);
        create_pool(reader_url).await?
    } else {
        writer.clone()
    };

    let pool_set = Arc::new(DbPoolSet::new(writer.clone(), reader));

    info!("Database pool created successfully");

    Ok((writer, pool_set))
}

pub async fn run_migrations(pool: &DbPool) -> anyhow::Result<()> {
    info!("Running database migrations");

    if std::env::var("DEKO_SKIP_MIGRATIONS").is_ok() {
        info!("Skipping database migrations (DEKO_SKIP_MIGRATIONS is set)");
        return Ok(());
    }

    if let Ok(backup_dir) = std::env::var("DEKO_BACKUP_DIR") {
        backup_database(&backup_dir).await;
    }

    #[cfg(not(feature = "postgres"))]
    let result = sqlx::migrate!("./migrations").run(pool).await;

    #[cfg(feature = "postgres")]
    let result = sqlx::migrate!("./migrations_postgres").run(pool).await;

    match result {
        Ok(_) => {
            info!("Database migrations completed successfully");
            Ok(())
        }
        Err(e) => {
            error!("Migration failed: {}", e);
            anyhow::bail!(
                "Database migration failed: {}. To skip: set DEKO_SKIP_MIGRATIONS=1. \
                 To attempt rollback: create down migrations and set DEKO_MIGRATE_REVERT_ON_FAILURE=1. \
                 Error: {}",
                e, e
            );
        }
    }
}

/// Performs a file-level backup of the SQLite database before migrations.
/// For PostgreSQL, this is a no-op (use pg_dump externally).
async fn backup_database(backup_dir: &str) {
    let db_url = std::env::var("DEKO_DATABASE_URL").unwrap_or_default();

    if !db_url.starts_with("sqlite:") {
        info!("Skipping file backup for non-SQLite database. Use pg_dump or your preferred tool.");
        return;
    }

    let db_path = db_url.trim_start_matches("sqlite://").trim_start_matches("sqlite:");
    let db_path = if db_path.is_empty() { "data/deko.db" } else { db_path };

    let path = std::path::Path::new(db_path);
    if !path.exists() {
        info!("Database file not found at {}, skipping backup", db_path);
        return;
    }

    let backup_name = format!(
        "deko_backup_{}.db",
        chrono::Utc::now().format("%Y%m%d_%H%M%S")
    );
    let backup_path = std::path::Path::new(backup_dir).join(&backup_name);

    if let Err(e) = std::fs::create_dir_all(backup_dir) {
        error!("Failed to create backup directory {}: {}", backup_dir, e);
        return;
    }

    match std::fs::copy(path, &backup_path) {
        Ok(size) => info!("Database backed up to {} ({} bytes)", backup_path.display(), size),
        Err(e) => error!("Database backup failed: {}", e),
    }
}
