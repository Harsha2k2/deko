mod config;
mod db;
mod error;
mod models;
mod middleware;
mod routes;
mod services;

use std::net::SocketAddr;
use std::sync::Arc;

use config::{Config, init_tracing};
use db::{init_db, run_migrations};
use routes::create_router;
use services::{ActionProcessor, VerdictService, MetricsCollector};
use tracing::info;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let config = Config::from_env()?;
    init_tracing(&config.env);

    info!("Starting Deko v{}", env!("CARGO_PKG_VERSION"));
    info!("Environment: {}", config.env);

    let pool = init_db(&config).await?;
    run_migrations(&pool).await?;

    let metrics = Arc::new(MetricsCollector::new());
    let verdict_service = VerdictService::new(pool.clone(), &config, metrics.clone());
    let processor = ActionProcessor::new(pool.clone(), verdict_service, config.processor_poll_interval_secs);
    let shutdown = processor.shutdown.clone();

    let processor_handle = tokio::spawn(async move {
        processor.run().await;
    });

    let app = create_router(&config, pool.clone())?;

    let addr: SocketAddr = config.addr();
    info!("Server listening on {}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await?;

    let server_handle = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    let shutdown_signal = tokio::signal::ctrl_c();
    shutdown_signal.await.expect("Failed to install signal handler");

    info!("Shutdown signal received");
    shutdown.store(true, std::sync::atomic::Ordering::Relaxed);

    processor_handle.abort();
    server_handle.abort();

    info!("Deko shut down gracefully");

    Ok(())
}
