//! Web server — Axum router + shared state.
//! 1:1 port of Python server.py.

pub mod api;
pub mod ws;

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use axum::Router;
use tokio::sync::RwLock;
use tower_http::cors::CorsLayer;
use tower_http::services::{ServeDir, ServeFile};

use anemone_core::brain::Brain;

/// Shared application state — all brains keyed by anemone ID.
pub struct AppState {
    pub brains: RwLock<HashMap<String, Arc<RwLock<Brain>>>>,
    pub project_root: PathBuf,
}

pub fn router(state: Arc<AppState>) -> Router {
    let cors = CorsLayer::very_permissive();

    let mut app = Router::new()
        .merge(api::routes())
        .merge(ws::routes())
        .layer(cors)
        .with_state(state.clone());

    // Serve frontend static files if dist directory exists
    let frontend_dist = state.project_root.join("crates/anemone-web/frontend/dist");
    if frontend_dist.is_dir() {
        let index_html = frontend_dist.join("index.html");
        app = app.fallback_service(
            ServeDir::new(&frontend_dist)
                .not_found_service(ServeFile::new(index_html)),
        );
    }

    app
}
