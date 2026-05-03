use sqlx::SqlitePool;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::time::{interval, Duration};
use tracing::{error, info};

use crate::services::verdict::VerdictService;

pub struct ActionProcessor {
    pub pool: SqlitePool,
    pub verdict_service: VerdictService,
    pub interval_secs: u64,
    pub shutdown: Arc<AtomicBool>,
}

impl ActionProcessor {
    pub fn new(pool: SqlitePool, verdict_service: VerdictService, interval_secs: u64) -> Self {
        Self {
            pool,
            verdict_service,
            interval_secs,
            shutdown: Arc::new(AtomicBool::new(false)),
        }
    }

    pub async fn run(&self) {
        let mut interval = interval(Duration::from_secs(self.interval_secs));

        loop {
            interval.tick().await;

            if self.shutdown.load(Ordering::Relaxed) {
                info!("Action processor shutting down");
                break;
            }

            if let Err(e) = self.process_pending().await {
                error!("Error processing pending actions: {}", e);
            }
        }
    }

    async fn process_pending(&self) -> anyhow::Result<()> {
        let pending_actions: Vec<String> = sqlx::query_scalar(
            "SELECT id FROM actions WHERE status = 'pending' ORDER BY created_at ASC LIMIT 10",
        )
        .fetch_all(&self.pool)
        .await?;

        if pending_actions.is_empty() {
            return Ok(());
        }

        info!("Processing {} pending actions", pending_actions.len());

        for action_id in pending_actions {
            if self.shutdown.load(Ordering::Relaxed) {
                break;
            }

            if let Err(e) = self.verdict_service.process_action(&action_id).await {
                error!("Failed to process action {}: {}", action_id, e);
                sqlx::query("UPDATE actions SET status = 'denied', updated_at = datetime('now') WHERE id = ?")
                    .bind(&action_id)
                    .execute(&self.pool)
                    .await?;
            }
        }

        Ok(())
    }
}
