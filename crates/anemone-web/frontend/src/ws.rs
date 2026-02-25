//! WebSocket client — connects to the Axum backend for real-time events.

use gloo_net::websocket::{futures::WebSocket, Message};
use futures::StreamExt;
use serde_json::Value;
use wasm_bindgen::JsCast;

/// Connect to the WebSocket for a given anemone and call handler on each event.
pub async fn connect_ws<F>(anemone_id: &str, mut on_event: F)
where
    F: FnMut(Value) + 'static,
{
    let window = web_sys::window().expect("no window");
    let location = window.location();
    let protocol = if location.protocol().unwrap_or_default() == "https:" {
        "wss:"
    } else {
        "ws:"
    };
    let host = location.host().unwrap_or_else(|_| "localhost:8000".to_string());
    let url = format!("{}//{}/ws/{}", protocol, host, anemone_id);

    match WebSocket::open(&url) {
        Ok(ws) => {
            let (_write, mut read) = ws.split();
            while let Some(msg) = read.next().await {
                match msg {
                    Ok(Message::Text(text)) => {
                        if let Ok(val) = serde_json::from_str::<Value>(&text) {
                            on_event(val);
                        }
                    }
                    Ok(Message::Bytes(_)) => {}
                    Err(_) => break,
                }
            }
        }
        Err(e) => {
            web_sys::console::error_1(
                &format!("WebSocket connect failed: {:?}", e).into(),
            );
        }
    }
}
