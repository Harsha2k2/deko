use sqlx::{sqlite::SqlitePoolOptions, SqlitePool};
use tracing::info;

use crate::config::Config;

pub async fn init_db(config: &Config) -> anyhow::Result<SqlitePool> {
    info!("Initializing database connection");

    let pool = SqlitePoolOptions::new()
        .max_connections(10)
        .acquire_timeout(std::time::Duration::from_secs(5))
        .connect(&config.database_url)
        .await?;

    info!("Database pool created successfully");

    Ok(pool)
}

pub async fn run_migrations(pool: &SqlitePool) -> anyhow::Result<()> {
    info!("Running database migrations");

    sqlx::migrate!("./migrations").run(pool).await?;

    info!("Database migrations completed successfully");

    Ok(())
}
