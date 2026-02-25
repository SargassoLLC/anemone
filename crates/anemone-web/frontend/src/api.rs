//! HTTP fetch calls to the Axum backend.

use gloo_net::http::Request;
use serde_json::Value;

use crate::AnemoneInfo;

pub async fn fetch_anemones() -> Result<Vec<AnemoneInfo>, String> {
    let resp = Request::get("/api/anemones")
        .send()
        .await
        .map_err(|e| e.to_string())?;
    resp.json().await.map_err(|e| e.to_string())
}

pub async fn fetch_status(anemone_id: &str) -> Result<Value, String> {
    let resp = Request::get(&format!("/api/status?anemone={}", anemone_id))
        .send()
        .await
        .map_err(|e| e.to_string())?;
    resp.json().await.map_err(|e| e.to_string())
}

pub async fn send_message(anemone_id: &str, text: &str) -> Result<(), String> {
    let body = serde_json::json!({"text": text});
    Request::post(&format!("/api/message?anemone={}", anemone_id))
        .header("Content-Type", "application/json")
        .body(body.to_string())
        .map_err(|e| e.to_string())?
        .send()
        .await
        .map_err(|e| e.to_string())?;
    Ok(())
}
