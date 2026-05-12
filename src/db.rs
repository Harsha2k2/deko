#[cfg(not(feature = "postgres"))]
use sqlx::{sqlite::SqlitePoolOptions, SqlitePool};
#[cfg(feature = "postgres")]
use sqlx::{postgres::PgPoolOptions, PgPool};
use tracing::info;

use crate::config::Config;

#[cfg(not(feature = "postgres"))]
pub type DbPool = SqlitePool;
#[cfg(feature = "postgres")]
pub type DbPool = PgPool;

pub async fn init_db(config: &Config) -> anyhow::Result<DbPool> {
    info!("Initializing database connection");

    #[cfg(not(feature = "postgres"))]
    let pool = SqlitePoolOptions::new()
        .max_connections(10)
        .acquire_timeout(std::time::Duration::from_secs(5))
        .connect(&config.database_url)
        .await?;

    #[cfg(feature = "postgres")]
    let pool = PgPoolOptions::new()
        .max_connections(10)
        .acquire_timeout(std::time::Duration::from_secs(5))
        .connect(&config.database_url)
        .await?;

    info!("Database pool created successfully");

    Ok(pool)
}

pub async fn run_migrations(pool: &DbPool) -> anyhow::Result<()> {
    info!("Running database migrations");

    #[cfg(not(feature = "postgres"))]
    sqlx::migrate!("./migrations").run(pool).await?;

    #[cfg(feature = "postgres")]
    sqlx::migrate!("./migrations_postgres").run(pool).await?;

    info!("Database migrations completed successfully");

    Ok(())
}
