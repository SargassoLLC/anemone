//! WebSocket — broadcast brain events to connected clients.
//! 1:1 port of Python server.py websocket logic.

use std::sync::Arc;

use axum::{
    extract::{ws::WebSocket, Path, State, WebSocketUpgrade},
    response::IntoResponse,
    routing::get,
    Router,
};
use tracing::{error, info};

use super::AppState;

pub fn routes() -> Router<Arc<AppState>> {
    Router::new()
        .route("/ws/{anemone_id}", get(ws_handler))
        .route("/ws", get(ws_default))
}

async fn ws_handler(
    ws: WebSocketUpgrade,
    Path(anemone_id): Path<String>,
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_socket(socket, anemone_id, state))
}

async fn ws_default(
    ws: WebSocketUpgrade,
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| async move {
        let first_id = {
            let brains = state.brains.read().await;
            brains.keys().next().cloned()
        };
        match first_id {
            Some(id) => handle_socket(socket, id, state).await,
            None => {
                error!("No brains available for default WebSocket");
                drop(socket);
            }
        }
    })
}

async fn handle_socket(mut socket: WebSocket, anemone_id: String, state: Arc<AppState>) {
    let brain_arc = {
        let brains = state.brains.read().await;
        brains.get(&anemone_id).cloned()
    };

    let brain_arc = match brain_arc {
        Some(b) => b,
        None => {
            error!("WebSocket: anemone '{}' not found", anemone_id);
            drop(socket);
            return;
        }
    };

    // Subscribe to brain events
    let mut rx = {
        let brain = brain_arc.read().await;
        brain.subscribe()
    };

    info!("WebSocket client connected to {}", anemone_id);

    // Forward events to the WebSocket client
    loop {
        tokio::select! {
            // Incoming events from the brain -> send to client
            result = rx.recv() => {
                match result {
                    Ok(event) => {
                        match serde_json::to_string(&event) {
                            Ok(json) => {
                                if socket.send(axum::extract::ws::Message::Text(json.into())).await.is_err() {
                                    break;
                                }
                            }
                            Err(e) => {
                                error!("Failed to serialize event: {}", e);
                            }
                        }
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                        info!("WebSocket lagged {} events", n);
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                        break;
                    }
                }
            }
            // Incoming messages from client (keep-alive)
            msg = socket.recv() => {
                match msg {
                    Some(Ok(_)) => {} // keep-alive, ignore content
                    _ => break,       // disconnected or error
                }
            }
        }
    }

    info!("WebSocket client disconnected from {}", anemone_id);
}
