use axum::extract::ws::{Message, Utf8Bytes, WebSocket, WebSocketUpgrade};
use axum::extract::{Path, State};
use axum::response::IntoResponse;
use std::time::Duration;
use tokio::time::sleep;

use crate::db::DbPool;
use crate::models::Agent;

fn json_msg(val: serde_json::Value) -> Message {
    Message::Text(Utf8Bytes::from(val.to_string()))
}

pub async fn action_ws_handler(
    ws: WebSocketUpgrade,
    State(pool): State<DbPool>,
    axum::Extension(agent): axum::Extension<Agent>,
    Path(action_id): Path<String>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_socket(socket, pool, agent, action_id))
}

async fn handle_socket(
    mut socket: WebSocket,
    pool: DbPool,
    agent: Agent,
    action_id: String,
) {
    let valid = sqlx::query_as::<_, (String,)>(
        "SELECT agent_id FROM actions WHERE id = ?",
    )
    .bind(&action_id)
    .fetch_optional(&pool)
    .await;

    let action_owner = match valid {
        Ok(Some((aid,))) => aid,
        _ => {
            let _ = socket.send(json_msg(serde_json::json!({"error": "Action not found"}))).await;
            return;
        }
    };

    if action_owner != agent.id {
        let _ = socket.send(json_msg(serde_json::json!({"error": "Forbidden"}))).await;
        return;
    }

    let max_attempts = 120;
    for attempt in 0..max_attempts {
        let verdict = sqlx::query_as::<_, (String, String, Option<String>, Option<String>)>(
            "SELECT decision, reason, risk_level, policy_matched FROM verdicts WHERE action_id = ?",
        )
        .bind(&action_id)
        .fetch_optional(&pool)
        .await;

        match verdict {
            Ok(Some((decision, reason, risk_level, policy_matched))) => {
                let msg = serde_json::json!({
                    "status": "completed",
                    "decision": decision,
                    "reason": reason,
                    "risk_level": risk_level,
                    "policy_matched": policy_matched,
                });
                let _ = socket.send(json_msg(msg)).await;
                return;
            }
            Ok(None) => {
                if attempt == 0 {
                    let _ = socket.send(json_msg(serde_json::json!({"status": "pending"}))).await;
                }
                sleep(Duration::from_millis(500)).await;
            }
            Err(_) => {
                let _ = socket.send(json_msg(serde_json::json!({"error": "Database error"}))).await;
                return;
            }
        }
    }

    let _ = socket.send(json_msg(serde_json::json!({
        "status": "timeout",
        "message": "Verdict not ready within 60 seconds"
    }))).await;
}
