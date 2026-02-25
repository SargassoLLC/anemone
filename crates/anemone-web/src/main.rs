//! anemone-web — Axum web server entry point.
//! Multi-anemone discovery + brain startup + serves API.
//! 1:1 port of Python main.py + server.py startup logic.

mod server;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use tokio::sync::RwLock;
use tracing::info;

use anemone_core::brain::Brain;
use anemone_core::config::Config;
use anemone_core::identity;

use server::AppState;

/// Derive anemone ID from box directory name: coral_box -> coral.
fn anemone_id_from_box(box_path: &Path) -> String {
    let dirname = box_path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("anemone");
    if let Some(stripped) = dirname.strip_suffix("_box") {
        stripped.to_string()
    } else {
        dirname.to_string()
    }
}

/// Discover all *_box/ directories with valid identity.json.
fn discover_anemones(
    project_root: &Path,
    config: &Config,
) -> HashMap<String, Arc<RwLock<Brain>>> {
    let mut brains = HashMap::new();

    let mut boxes: Vec<PathBuf> = Vec::new();
    if let Ok(entries) = std::fs::read_dir(project_root) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                    if name.ends_with("_box") {
                        boxes.push(path);
                    }
                }
            }
        }
    }
    boxes.sort();

    for box_path in &boxes {
        match identity::load_identity_from(box_path) {
            Ok(Some(ident)) => {
                let anemone_id = anemone_id_from_box(box_path);
                let brain = Brain::new(ident, box_path.clone(), config.clone());
                brains.insert(anemone_id, Arc::new(RwLock::new(brain)));
            }
            Ok(None) => {
                info!("Skipping {:?} — no identity.json", box_path);
            }
            Err(e) => {
                info!("Skipping {:?} — invalid identity: {}", box_path, e);
            }
        }
    }

    brains
}

#[tokio::main]
async fn main() {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info".into()),
        )
        .init();

    // Determine project root (parent of the binary or current dir)
    let project_root = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));

    let config_path = project_root.join("config.yaml");
    let config = Config::load(&config_path).unwrap_or_default();

    // Discover anemones
    let brains = discover_anemones(&project_root, &config);

    if brains.is_empty() {
        eprintln!("\n  No anemones found (no *_box/ directories with identity.json).");
        eprintln!("  Create one by sending POST /api/anemones with {{\"name\": \"YourName\"}}");
        eprintln!("  Or create a directory like 'coral_box/' and run the onboarding.\n");
    } else {
        let names: Vec<String> = {
            let mut names = Vec::new();
            for (id, brain_arc) in &brains {
                let brain = brain_arc.read().await;
                names.push(format!("{} ({})", brain.identity.name, id));
            }
            names
        };
        eprintln!("\n  Found {} anemone(s): {}", brains.len(), names.join(", "));
    }

    let state = Arc::new(AppState {
        brains: RwLock::new(brains),
        project_root: project_root.clone(),
    });

    // Start all brains
    {
        let brains = state.brains.read().await;
        for (anemone_id, brain_arc) in brains.iter() {
            let brain_for_task = Arc::clone(brain_arc);
            let id = anemone_id.clone();
            tokio::spawn(async move {
                // Small delay so the server binds port first
                tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                let mut brain = brain_for_task.write().await;
                info!("{} ({}) starting...", brain.identity.name, id);
                brain.run().await;
            });
        }
    }

    let state_for_shutdown = Arc::clone(&state);
    let app = server::router(state);

    let port: u16 = std::env::var("PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(8000);
    let addr = format!("0.0.0.0:{}", port);

    eprintln!("  Open http://localhost:{} to watch them think\n", port);

    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .expect("Failed to bind port");

    // Graceful shutdown on Ctrl+C
    let shutdown = async move {
        tokio::signal::ctrl_c()
            .await
            .expect("Failed to install Ctrl+C handler");
        info!("Shutdown signal received, stopping brains...");

        // Send stop command to all brains
        let brains = state_for_shutdown.brains.read().await;
        for (id, brain_arc) in brains.iter() {
            let brain = brain_arc.read().await;
            let _ = brain
                .command_tx
                .send(anemone_core::brain::BrainCommand::Stop)
                .await;
            info!("{} stopping...", id);
        }
    };

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown)
        .await
        .expect("Server error");

    info!("Server stopped.");
}
