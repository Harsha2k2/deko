mod config;
mod db;
mod error;
mod middleware;
mod models;
mod routes;
mod services;

use std::net::SocketAddr;
use std::sync::Arc;

use config::{init_tracing, Config};
use db::{init_db, run_migrations};

#[allow(unused_imports)]
use dotenvy::dotenv;
use routes::create_router;
use services::{ws_broadcaster::WsBroadcaster, ActionProcessor, MetricsCollector, VerdictService};
use tracing::{error, info, warn};

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
    // hydrate process env from .env if present; real env always wins
    // (kept out of Config::from_env so tests never depend on cwd files)
    dotenvy::dotenv().ok();
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
            use tokio::signal::unix::{signal, SignalKind};
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

    // chain any audit rows written before the hash columns existed
    match services::audit::backfill_unchained(&pool).await {
        Ok(n) if n > 0 => info!("audit chain: backfilled {} legacy entries", n),
        Ok(_) => {}
        Err(e) => {
            // fail closed on tamper-evidence setup: refuse to run with a
            // broken audit trail
            error!("audit chain backfill failed: {}", e);
            return Err(e.into());
        }
    }

    match services::session::purge_expired(&pool).await {
        Ok(n) if n > 0 => info!("purged {} expired admin sessions", n),
        Ok(_) => {}
        Err(e) => warn!("admin session purge failed: {}", e),
    }

    let metrics = Arc::new(MetricsCollector::new());
    metrics.set_pool_config(10, 5);
    let ws_broadcaster = Arc::new(WsBroadcaster::new(256));
    let verdict_service = Arc::new(VerdictService::new(
        pool.clone(),
        &config,
        metrics.clone(),
        ws_broadcaster.clone(),
    ));
    verdict_service.start_health_checks(60);
    let processor = ActionProcessor::new(
        pool.clone(),
        verdict_service.clone(),
        config.processor_poll_interval_secs,
        config.action_ttl_secs,
        10,
    );
    let processor_shutdown = processor.shutdown.clone();

    let processor_handle = tokio::spawn(async move {
        processor.run().await;
    });

    let app = create_router(&config, pool.clone(), pool_set, ws_broadcaster.clone())?;

    let addr: SocketAddr = config.addr();
    info!("Server listening on {}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await?;

    // ctrl-c or SIGTERM (k8s/docker stop) both drain in-flight requests
    let shutdown_signal = async {
        let ctrl_c = tokio::signal::ctrl_c();
        #[cfg(unix)]
        {
            let mut sigterm = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
                .expect("failed to install SIGTERM handler");
            tokio::select! {
                _ = ctrl_c => {},
                _ = sigterm.recv() => {},
            }
        }
        #[cfg(not(unix))]
        {
            ctrl_c.await.expect("Failed to install signal handler");
        }
    };

    axum::serve(listener, app)
        .with_graceful_shutdown(async move {
            shutdown_signal.await;
            info!("shutdown signal received; draining in-flight requests");
            // stop the processor loop first so no new work is claimed while
            // the http layer finishes its in-flight responses
            processor_shutdown.store(true, std::sync::atomic::Ordering::Relaxed);
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        })
        .await?;

    info!("server drained; stopping background processor");
    processor_handle.abort();

    info!("deko shut down gracefully");

    Ok(())
}
