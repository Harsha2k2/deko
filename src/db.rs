use sqlx::{sqlite::SqlitePoolOptions, SqlitePool};
use std::ops::Deref;
use std::sync::Arc;
use tracing::{error, info};

use crate::config::Config;

pub type DbPool = SqlitePool;

/// wraps writer and optional reader pools for future read-replica support.
/// today every reader call resolves to the single writer pool; the type is
/// kept so query routing can be introduced without touching signatures.
#[derive(Clone)]
pub struct DbPoolSet {
    writer: DbPool,
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

/// auto-deref to the writer pool so &DbPoolSet can be used with sqlx.
impl Deref for DbPoolSet {
    type Target = DbPool;

    fn deref(&self) -> &Self::Target {
        self.writer()
    }
}

async fn create_pool(url: &str) -> anyhow::Result<DbPool> {
    let pool = SqlitePoolOptions::new()
        .max_connections(10)
        .acquire_timeout(std::time::Duration::from_secs(5))
        .connect(url)
        .await?;
    Ok(pool)
}

pub async fn init_db(config: &Config) -> anyhow::Result<(DbPool, Arc<DbPoolSet>)> {
    info!("Initializing database connection");

    // wal mode for file dbs; shared cache for in-memory dbs so all pool
    // connections see the same database (otherwise each connection gets a
    // private :memory: db and the processor never sees the handler's rows)
    let url = if config.database_url.contains(":memory:") {
        "sqlite::memory:?cache=shared".to_string()
    } else if config.database_url.starts_with("sqlite:") && !config.database_url.contains('?') {
        format!("{}?mode=rwc", config.database_url)
    } else {
        config.database_url.clone()
    };

    let writer = create_pool(&url).await?;

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

    let result = sqlx::migrate!("./migrations").run(pool).await;

    match result {
        Ok(_) => {
            info!("Database migrations completed successfully");
            Ok(())
        }
        Err(e) => {
            error!("Migration failed: {}", e);
            anyhow::bail!(
                "Database migration failed: {}. To skip: set DEKO_SKIP_MIGRATIONS=1. Error: {}",
                e,
                e
            );
        }
    }
}

/// performs a file-level backup of the sqlite database before migrations.
async fn backup_database(backup_dir: &str) {
    let db_url = std::env::var("DEKO_DATABASE_URL").unwrap_or_default();

    let db_path = db_url.trim_start_matches("sqlite://").trim_start_matches("sqlite:");
    let db_path = db_path.split('?').next().unwrap_or(db_path);
    let db_path = if db_path.is_empty() { "data/deko.db" } else { db_path };

    let path = std::path::Path::new(db_path);
    if !path.exists() {
        info!("Database file not found at {}, skipping backup", db_path);
        return;
    }

    let backup_name = format!("deko_backup_{}.db", chrono::Utc::now().format("%Y%m%d_%H%M%S"));
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
