use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use axum::body::Body;
use axum::extract::Request;
use axum::http::StatusCode;
use axum::middleware::Next;
use axum::response::IntoResponse;
use serde::Serialize;
use tokio::time::interval;

/// Thread-safe metrics collector using atomic counters.
///
/// Tracks action counts, LLM call latency, webhook delivery status, and HTTP
/// request metrics. Exposed at the `GET /metrics` endpoint as JSON.
///
/// All counters use `AtomicU64` for lock-free concurrent access.
#[derive(Clone)]
pub struct MetricsCollector {
    pub actions_total: Arc<AtomicU64>,
    pub actions_approved: Arc<AtomicU64>,
    pub actions_denied: Arc<AtomicU64>,
    pub actions_escalated: Arc<AtomicU64>,
    pub llm_calls_total: Arc<AtomicU64>,
    pub llm_errors: Arc<AtomicU64>,
    pub llm_latency_ms: Arc<AtomicU64>,
    pub webhook_sent: Arc<AtomicU64>,
    pub webhook_failed: Arc<AtomicU64>,
    pub request_duration_ms: Arc<AtomicU64>,
    pub request_count: Arc<AtomicU64>,
    pub pool_max_connections: Arc<AtomicU32>,
    pub pool_acquire_timeout_secs: Arc<AtomicU64>,
}

impl Default for MetricsCollector {
    fn default() -> Self {
        Self::new()
    }
}

impl MetricsCollector {
    pub fn new() -> Self {
        Self {
            actions_total: Arc::new(AtomicU64::new(0)),
            actions_approved: Arc::new(AtomicU64::new(0)),
            actions_denied: Arc::new(AtomicU64::new(0)),
            actions_escalated: Arc::new(AtomicU64::new(0)),
            llm_calls_total: Arc::new(AtomicU64::new(0)),
            llm_errors: Arc::new(AtomicU64::new(0)),
            llm_latency_ms: Arc::new(AtomicU64::new(0)),
            webhook_sent: Arc::new(AtomicU64::new(0)),
            webhook_failed: Arc::new(AtomicU64::new(0)),
            request_duration_ms: Arc::new(AtomicU64::new(0)),
            request_count: Arc::new(AtomicU64::new(0)),
            pool_max_connections: Arc::new(AtomicU32::new(10)),
            pool_acquire_timeout_secs: Arc::new(AtomicU64::new(5)),
        }
    }

    pub fn set_pool_config(&self, max_connections: u32, acquire_timeout_secs: u64) {
        self.pool_max_connections.store(max_connections, Ordering::Relaxed);
        self.pool_acquire_timeout_secs.store(acquire_timeout_secs, Ordering::Relaxed);
    }

    pub fn inc_action(&self) {
        self.actions_total.fetch_add(1, Ordering::Relaxed);
    }

    pub fn inc_action_approved(&self) {
        self.actions_approved.fetch_add(1, Ordering::Relaxed);
    }

    pub fn inc_action_denied(&self) {
        self.actions_denied.fetch_add(1, Ordering::Relaxed);
    }

    pub fn inc_action_escalated(&self) {
        self.actions_escalated.fetch_add(1, Ordering::Relaxed);
    }

    pub fn inc_llm_call(&self) {
        self.llm_calls_total.fetch_add(1, Ordering::Relaxed);
    }

    pub fn inc_llm_error(&self) {
        self.llm_errors.fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_llm_latency(&self, ms: u64) {
        let old = self.llm_latency_ms.load(Ordering::Relaxed);
        let count = self.llm_calls_total.load(Ordering::Relaxed).max(1);
        let new_avg = ((old * (count - 1)) + ms) / count;
        self.llm_latency_ms.store(new_avg, Ordering::Relaxed);
    }

    #[allow(dead_code)]
    pub fn inc_webhook_sent(&self) {
        self.webhook_sent.fetch_add(1, Ordering::Relaxed);
    }

    #[allow(dead_code)]
    pub fn inc_webhook_failed(&self) {
        self.webhook_failed.fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_request(&self, ms: u64) {
        self.request_count.fetch_add(1, Ordering::Relaxed);
        let old = self.request_duration_ms.load(Ordering::Relaxed);
        let count = self.request_count.load(Ordering::Relaxed).max(1);
        let new_avg = ((old * (count - 1)) + ms) / count;
        self.request_duration_ms.store(new_avg, Ordering::Relaxed);
    }

    pub fn to_json(&self) -> serde_json::Value {
        serde_json::json!({
            "actions": {
                "total": self.actions_total.load(Ordering::Relaxed),
                "approved": self.actions_approved.load(Ordering::Relaxed),
                "denied": self.actions_denied.load(Ordering::Relaxed),
                "escalated": self.actions_escalated.load(Ordering::Relaxed),
            },
            "llm": {
                "calls_total": self.llm_calls_total.load(Ordering::Relaxed),
                "errors": self.llm_errors.load(Ordering::Relaxed),
                "avg_latency_ms": self.llm_latency_ms.load(Ordering::Relaxed),
            },
            "webhooks": {
                "sent": self.webhook_sent.load(Ordering::Relaxed),
                "failed": self.webhook_failed.load(Ordering::Relaxed),
            },
            "http": {
                "request_count": self.request_count.load(Ordering::Relaxed),
                "avg_duration_ms": self.request_duration_ms.load(Ordering::Relaxed),
            },
            "database": {
                "pool_max_connections": self.pool_max_connections.load(Ordering::Relaxed),
                "pool_acquire_timeout_secs": self.pool_acquire_timeout_secs.load(Ordering::Relaxed),
            }
        })
    }
}

#[derive(Clone)]
pub struct RateLimiter {
    state: Arc<Mutex<HashMap<String, RateLimitState>>>,
    max_requests: u64,
    window_secs: u64,
}

#[derive(Debug)]
struct RateLimitState {
    count: u64,
    window_start: Instant,
}

impl RateLimiter {
    pub fn new(max_requests: u64, window_secs: u64) -> Self {
        Self {
            state: Arc::new(Mutex::new(HashMap::new())),
            max_requests,
            window_secs,
        }
    }

    #[allow(dead_code)]
    pub fn start_cleanup_task(&self) {
        let state_clone = self.state.clone();
        let window = Duration::from_secs(self.window_secs);
        tokio::spawn(async move {
            let mut interval = interval(window);
            loop {
                interval.tick().await;
                if let Ok(mut map) = state_clone.lock() {
                    let now = Instant::now();
                    map.retain(|_, state| now.duration_since(state.window_start) < window);
                }
            }
        });
    }

    pub fn is_allowed(&self, key: &str) -> bool {
        let mut map = self.state.lock().unwrap();
        let now = Instant::now();
        let window = Duration::from_secs(self.window_secs);

        let entry = map.entry(key.to_string()).or_insert(RateLimitState {
            count: 0,
            window_start: now,
        });

        if now.duration_since(entry.window_start) > window {
            entry.count = 0;
            entry.window_start = now;
        }

        if entry.count >= self.max_requests {
            false
        } else {
            entry.count += 1;
            true
        }
    }
}

pub async fn rate_limit_middleware(
    axum::Extension(limiter): axum::Extension<RateLimiter>,
    request: Request<Body>,
    next: Next,
) -> impl IntoResponse {
    let client_ip = request
        .headers()
        .get("x-forwarded-for")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("unknown");

    if !limiter.is_allowed(client_ip) {
        return (StatusCode::TOO_MANY_REQUESTS, axum::Json(serde_json::json!({
            "error": "Rate limit exceeded"
        }))).into_response();
    }

    next.run(request).await
}

pub async fn request_metrics_middleware(
    axum::Extension(metrics): axum::Extension<MetricsCollector>,
    request: Request<Body>,
    next: Next,
) -> impl IntoResponse {
    let start = Instant::now();
    let response = next.run(request).await;
    let elapsed = start.elapsed().as_millis() as u64;
    metrics.record_request(elapsed);
    response
}

#[allow(dead_code)]
#[derive(Serialize)]
pub struct MetricsResponse {
    pub actions: ActionMetrics,
    pub llm: LLMMetrics,
    pub webhooks: WebhookMetrics,
    pub http: HTTPMetrics,
}

#[allow(dead_code)]
#[derive(Serialize)]
pub struct ActionMetrics {
    pub total: u64,
    pub approved: u64,
    pub denied: u64,
    pub escalated: u64,
}

#[allow(dead_code)]
#[derive(Serialize)]
pub struct LLMMetrics {
    pub calls_total: u64,
    pub errors: u64,
    pub avg_latency_ms: u64,
}

#[allow(dead_code)]
#[derive(Serialize)]
pub struct WebhookMetrics {
    pub sent: u64,
    pub failed: u64,
}

#[allow(dead_code)]
#[derive(Serialize)]
pub struct HTTPMetrics {
    pub request_count: u64,
    pub avg_duration_ms: u64,
}
