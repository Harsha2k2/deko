use std::sync::Arc;
use crate::db::DbPool;
use std::sync::atomic::{AtomicBool, Ordering};
use tokio::time::{interval, Duration};
use tracing::{error, info};

use crate::services::verdict::VerdictService;

pub struct ActionProcessor {
    pub pool: DbPool,
    pub verdict_service: VerdictService,
    pub interval_secs: u64,
    pub action_ttl_secs: u64,
    pub batch_size: u32,
    pub processing_timeout_secs: u64,
    pub shutdown: Arc<AtomicBool>,
}

impl ActionProcessor {
    pub fn new(pool: DbPool, verdict_service: VerdictService, interval_secs: u64, action_ttl_secs: u64, batch_size: u32, processing_timeout_secs: u64) -> Self {
        Self {
            pool,
            verdict_service,
            interval_secs,
            action_ttl_secs,
            batch_size,
            processing_timeout_secs,
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

            self.expire_stale_actions().await;

            if let Err(e) = self.process_pending().await {
                error!("Error processing pending actions: {}", e);
            }
        }
    }

    async fn expire_stale_actions(&self) {
        let cutoff = chrono::Utc::now() - chrono::Duration::seconds(self.action_ttl_secs as i64);
        let result = sqlx::query(
            "UPDATE actions SET status = 'denied', updated_at = CURRENT_TIMESTAMP \
             WHERE status = 'pending' AND created_at < ?",
        )
        .bind(cutoff.to_rfc3339())
        .execute(&self.pool)
        .await;

        if let Ok(r) = result {
            let expired = r.rows_affected();
            if expired > 0 {
                info!("Expired {} stale pending actions (TTL: {}s)", expired, self.action_ttl_secs);
            }
        }
    }

    async fn process_pending(&self) -> anyhow::Result<()> {
        let sql = format!(
            "SELECT id FROM actions WHERE status = 'pending' ORDER BY priority ASC, created_at ASC LIMIT {}",
            self.batch_size
        );
        let pending_actions: Vec<String> = sqlx::query_scalar(&sql)
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
                sqlx::query("UPDATE actions SET status = 'denied', updated_at = CURRENT_TIMESTAMP WHERE id = ?")
                    .bind(&action_id)
                    .execute(&self.pool)
                    .await?;
            }
        }

        Ok(())
    }
}
