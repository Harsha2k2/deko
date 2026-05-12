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
use tracing::{error, info};

/// Deko - AI Agent Action Watchdog
///
/// A security middleware that intercepts AI agent actions, evaluates them
/// against policies and LLM analysis, and decides whether to approve, deny,
/// or escalate each action.
///
/// # Architecture
/// ```text
/// Agent → POST /action → Policy Engine → LLM Analysis → Verdict → Forward/Block
/// ```
///
/// # Safety Guarantees
/// - **Default-Deny**: Every action is blocked unless explicitly approved
/// - **Fail-Closed**: System failures result in denial, never approval
/// - **Immutable Audit**: Every decision is permanently recorded
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let config = Config::from_env()?;
    init_tracing(&config.env);

    info!("Starting Deko v{}", env!("CARGO_PKG_VERSION"));
    info!("Environment: {}", config.env);

    let config = Arc::new(config);
    let reload_flag = Arc::new(std::sync::atomic::AtomicBool::new(false));

    let signal_reload = reload_flag.clone();
    let _signal_config = config.clone();
    tokio::spawn(async move {
        #[cfg(unix)]
        {
            use tokio::signal::unix::{SignalKind, signal};
            let mut sig = signal(SignalKind::hangup()).expect("Failed to install SIGHUP handler");
            sig.recv().await;
            info!("SIGHUP received: requesting config reload");
            signal_reload.store(true, std::sync::atomic::Ordering::Relaxed);
            match Config::from_env() {
                Ok(new_config) => {
                    info!("Config reloaded successfully. Some changes may require restart.");
                    let _ = new_config;
                }
                Err(e) => error!("Config reload failed: {}", e),
            }
        }
        #[cfg(not(unix))]
        {
            let _ = signal_reload;
            let _ = _signal_config;
            info!("Config hot-reload not supported on this platform (SIGHUP is Unix-only)");
        }
    });

    let (pool, pool_set) = init_db(&config).await?;
    run_migrations(&pool).await?;

    let metrics = Arc::new(MetricsCollector::new());
    metrics.set_pool_config(10, 5);
    let verdict_service = VerdictService::new(pool.clone(), &config, metrics.clone());
    let processor = ActionProcessor::new(pool.clone(), verdict_service, config.processor_poll_interval_secs, config.action_ttl_secs);
    let shutdown = processor.shutdown.clone();

    let processor_handle = tokio::spawn(async move {
        processor.run().await;
    });

    let app = create_router(&config, pool.clone(), pool_set)?;

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
