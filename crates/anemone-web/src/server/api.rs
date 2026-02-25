//! REST API endpoints — 1:1 port of Python server.py REST routes.

use std::sync::Arc;

use axum::{
    extract::{Path, Query, State},
    routing::{get, post},
    Json, Router,
};
use serde::Deserialize;
use serde_json::{json, Value};
use tracing::info;

use anemone_core::brain::{Brain, BrainCommand};
use anemone_core::config::Config;
use anemone_core::identity;

use super::AppState;

pub fn routes() -> Router<Arc<AppState>> {
    Router::new()
        .route("/api/anemones", get(list_anemones).post(create_anemone))
        .route("/api/identity", get(get_identity))
        .route("/api/events", get(get_events))
        .route("/api/raw", get(get_raw))
        .route("/api/status", get(get_status))
        .route("/api/focus-mode", post(post_focus_mode))
        .route("/api/message", post(post_message))
        .route("/api/snapshot", post(post_snapshot))
        .route("/api/files", get(get_files))
        .route("/api/files/{path:*}", get(get_file))
}

#[derive(Deserialize)]
struct AnemoneQuery {
    anemone: Option<String>,
}

#[derive(Deserialize)]
struct LimitQuery {
    anemone: Option<String>,
    limit: Option<usize>,
}

/// Resolve brain by ?anemone=ID query param, or default to first.
async fn resolve_brain(
    state: &AppState,
    anemone_id: Option<&str>,
) -> Option<(String, Arc<tokio::sync::RwLock<Brain>>)> {
    let brains = state.brains.read().await;
    if let Some(id) = anemone_id {
        brains
            .get(id)
            .map(|b| (id.to_string(), Arc::clone(b)))
    } else {
        brains
            .iter()
            .next()
            .map(|(id, b)| (id.clone(), Arc::clone(b)))
    }
}

// --- List anemones ---

async fn list_anemones(State(state): State<Arc<AppState>>) -> Json<Value> {
    let brains = state.brains.read().await;
    let mut list = Vec::new();
    for (anemone_id, brain_arc) in brains.iter() {
        let brain = brain_arc.read().await;
        list.push(json!({
            "id": anemone_id,
            "name": brain.identity.name,
            "state": brain.state,
            "thought_count": brain.thought_count,
        }));
    }
    Json(json!(list))
}

// --- Create anemone at runtime ---

#[derive(Deserialize)]
struct CreateBody {
    name: Option<String>,
}

async fn create_anemone(
    State(state): State<Arc<AppState>>,
    Json(body): Json<CreateBody>,
) -> Json<Value> {
    let name = match body.name.as_deref().map(|s| s.trim()) {
        Some(n) if !n.is_empty() => n.to_string(),
        _ => return Json(json!({"ok": false, "error": "name is required"})),
    };

    let anemone_id = name.to_lowercase();

    {
        let brains = state.brains.read().await;
        if brains.contains_key(&anemone_id) {
            return Json(json!({"ok": false, "error": format!("anemone '{}' already exists", anemone_id)}));
        }
    }

    let box_path = state.project_root.join(format!("{}_box", anemone_id));
    if let Err(e) = std::fs::create_dir_all(&box_path) {
        return Json(json!({"ok": false, "error": format!("Failed to create directory: {}", e)}));
    }

    // Create identity with random entropy
    let ident = identity::create_identity_random(&name);
    if let Err(e) = identity::save_identity(&ident, &box_path) {
        return Json(json!({"ok": false, "error": format!("Failed to save identity: {}", e)}));
    }

    let config_path = state.project_root.join("config.yaml");
    let config = Config::load(&config_path).unwrap_or_default();
    let brain = Brain::new(ident, box_path, config);
    let brain_arc = Arc::new(tokio::sync::RwLock::new(brain));

    // Start the brain
    let brain_for_task = Arc::clone(&brain_arc);
    tokio::spawn(async move {
        let mut brain = brain_for_task.write().await;
        brain.run().await;
    });

    {
        let mut brains = state.brains.write().await;
        brains.insert(anemone_id.clone(), brain_arc);
    }

    info!("Created and started new anemone: {} ({})", name, anemone_id);
    Json(json!({"ok": true, "id": anemone_id, "name": name}))
}

// --- Identity ---

async fn get_identity(
    State(state): State<Arc<AppState>>,
    Query(q): Query<AnemoneQuery>,
) -> Json<Value> {
    match resolve_brain(&state, q.anemone.as_deref()).await {
        Some((_, brain_arc)) => {
            let brain = brain_arc.read().await;
            Json(serde_json::to_value(&brain.identity).unwrap_or(json!({})))
        }
        None => Json(json!({"error": "no anemone found"})),
    }
}

// --- Events ---

async fn get_events(
    State(state): State<Arc<AppState>>,
    Query(q): Query<LimitQuery>,
) -> Json<Value> {
    let limit = q.limit.unwrap_or(100);
    match resolve_brain(&state, q.anemone.as_deref()).await {
        Some((_, brain_arc)) => {
            let brain = brain_arc.read().await;
            let start = brain.events.len().saturating_sub(limit);
            Json(json!(&brain.events[start..]))
        }
        None => Json(json!([])),
    }
}

// --- Raw API calls ---

async fn get_raw(
    State(state): State<Arc<AppState>>,
    Query(q): Query<LimitQuery>,
) -> Json<Value> {
    let limit = q.limit.unwrap_or(20);
    match resolve_brain(&state, q.anemone.as_deref()).await {
        Some((_, brain_arc)) => {
            let brain = brain_arc.read().await;
            let start = brain.api_calls.len().saturating_sub(limit);
            Json(json!(&brain.api_calls[start..]))
        }
        None => Json(json!([])),
    }
}

// --- Status ---

async fn get_status(
    State(state): State<Arc<AppState>>,
    Query(q): Query<AnemoneQuery>,
) -> Json<Value> {
    match resolve_brain(&state, q.anemone.as_deref()).await {
        Some((_, brain_arc)) => {
            let brain = brain_arc.read().await;
            Json(json!({
                "state": brain.state,
                "thought_count": brain.thought_count,
                "name": brain.identity.name,
                "position": brain.position,
                "focus_mode": false, // TODO: expose focus_mode field
            }))
        }
        None => Json(json!({"error": "no anemone found"})),
    }
}

// --- Focus mode ---

#[derive(Deserialize)]
struct FocusModeBody {
    enabled: Option<bool>,
}

async fn post_focus_mode(
    State(state): State<Arc<AppState>>,
    Query(q): Query<AnemoneQuery>,
    Json(body): Json<FocusModeBody>,
) -> Json<Value> {
    let enabled = body.enabled.unwrap_or(false);
    match resolve_brain(&state, q.anemone.as_deref()).await {
        Some((_, brain_arc)) => {
            let brain = brain_arc.read().await;
            let _ = brain
                .command_tx
                .send(BrainCommand::SetFocusMode(enabled))
                .await;
            Json(json!({"ok": true, "focus_mode": enabled}))
        }
        None => Json(json!({"ok": false, "error": "no anemone found"})),
    }
}

// --- Message ---

#[derive(Deserialize)]
struct MessageBody {
    text: Option<String>,
}

async fn post_message(
    State(state): State<Arc<AppState>>,
    Query(q): Query<AnemoneQuery>,
    Json(body): Json<MessageBody>,
) -> Json<Value> {
    let text = match body.text.as_deref().map(|s| s.trim()) {
        Some(t) if !t.is_empty() => t.to_string(),
        _ => return Json(json!({"ok": false, "error": "empty message"})),
    };

    match resolve_brain(&state, q.anemone.as_deref()).await {
        Some((_, brain_arc)) => {
            let brain = brain_arc.read().await;
            let cmd = if brain.is_waiting_for_reply() {
                BrainCommand::ConversationReply(text)
            } else {
                BrainCommand::UserMessage(text)
            };
            let _ = brain.command_tx.send(cmd).await;
            Json(json!({"ok": true}))
        }
        None => Json(json!({"ok": false, "error": "no anemone found"})),
    }
}

// --- Snapshot ---

#[derive(Deserialize)]
struct SnapshotBody {
    image: Option<String>,
}

async fn post_snapshot(
    State(state): State<Arc<AppState>>,
    Query(q): Query<AnemoneQuery>,
    Json(body): Json<SnapshotBody>,
) -> Json<Value> {
    if let Some(image) = body.image {
        match resolve_brain(&state, q.anemone.as_deref()).await {
            Some((_, brain_arc)) => {
                let brain = brain_arc.read().await;
                let _ = brain.command_tx.send(BrainCommand::Snapshot(image)).await;
                Json(json!({"ok": true}))
            }
            None => Json(json!({"ok": false, "error": "no anemone found"})),
        }
    } else {
        Json(json!({"ok": true}))
    }
}

// --- Files ---

async fn get_files(
    State(state): State<Arc<AppState>>,
    Query(q): Query<AnemoneQuery>,
) -> Json<Value> {
    match resolve_brain(&state, q.anemone.as_deref()).await {
        Some((_, brain_arc)) => {
            let brain = brain_arc.read().await;
            let env_root = brain
                .env_path
                .canonicalize()
                .unwrap_or_else(|_| brain.env_path.clone());
            let mut files: Vec<String> = Vec::new();
            collect_files(&env_root, &env_root, &mut files);
            files.sort();
            Json(json!({"files": files}))
        }
        None => Json(json!({"files": []})),
    }
}

fn collect_files(root: &std::path::Path, current: &std::path::Path, out: &mut Vec<String>) {
    if let Ok(entries) = std::fs::read_dir(current) {
        for entry in entries.flatten() {
            let path = entry.path();
            let name = entry.file_name();
            if name.to_string_lossy().starts_with('.') {
                continue;
            }
            if path.is_dir() {
                collect_files(root, &path, out);
            } else if let Ok(rel) = path.strip_prefix(root) {
                out.push(rel.to_string_lossy().to_string());
            }
        }
    }
}

async fn get_file(
    State(state): State<Arc<AppState>>,
    Query(q): Query<AnemoneQuery>,
    Path(path): Path<String>,
) -> Json<Value> {
    match resolve_brain(&state, q.anemone.as_deref()).await {
        Some((_, brain_arc)) => {
            let brain = brain_arc.read().await;
            let env_root = brain
                .env_path
                .canonicalize()
                .unwrap_or_else(|_| brain.env_path.clone());
            let full = env_root.join(&path);
            let full_real = full
                .canonicalize()
                .unwrap_or_else(|_| full.clone());

            if !full_real.starts_with(&env_root) {
                return Json(json!({"path": path, "content": "Blocked: path outside environment."}));
            }

            match std::fs::read_to_string(&full_real) {
                Ok(content) => Json(json!({"path": path, "content": content})),
                Err(e) => Json(json!({"path": path, "content": format!("Error: {}", e)})),
            }
        }
        None => Json(json!({"path": path, "content": "Error: no anemone found"})),
    }
}
