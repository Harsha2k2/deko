use std::sync::Arc;

use axum::extract::ws::{Message, Utf8Bytes, WebSocket, WebSocketUpgrade};
use axum::extract::Extension;
use axum::response::IntoResponse;
use tokio::select;

use crate::services::ws_broadcaster::WsBroadcaster;

pub async fn admin_ws_handler(
    ws: WebSocketUpgrade,
    Extension(broadcaster): Extension<Arc<WsBroadcaster>>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_admin_socket(socket, broadcaster))
}

async fn handle_admin_socket(mut socket: WebSocket, broadcaster: Arc<WsBroadcaster>) {
    let mut rx = broadcaster.subscribe();
    loop {
        select! {
            msg = rx.recv() => {
                match msg {
                    Ok(text) => {
                        if socket.send(Message::Text(Utf8Bytes::from(text))).await.is_err() {
                            break;
                        }
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                        let _ = socket.send(Message::Text(Utf8Bytes::from(
                            serde_json::json!({"type": "lag", "skipped": n}).to_string()
                        ))).await;
                    }
                    Err(_) => break,
                }
            }
            recv = socket.recv() => {
                // Client disconnected or sent a ping/pong; any message ends the session
                if recv.is_none() {
                    break;
                }
            }
        }
    }
}
