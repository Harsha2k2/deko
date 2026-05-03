mod config;
mod db;
mod error;
mod models;
mod middleware;
mod routes;
mod services;

use std::net::SocketAddr;

use config::{Config, init_tracing};
use db::{init_db, run_migrations};
use routes::create_router;
use tracing::info;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let config = Config::from_env()?;
    init_tracing(&config.env);

    info!("Starting Deko v{}", env!("CARGO_PKG_VERSION"));
    info!("Environment: {}", config.env);

    let pool = init_db(&config).await?;
    run_migrations(&pool).await?;

    let app = create_router(&config, pool)?;

    let addr: SocketAddr = config.addr();
    info!("Server listening on {}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await?;

    axum::serve(listener, app).await?;

    Ok(())
}
