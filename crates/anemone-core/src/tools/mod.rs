//! Tool execution — sandboxed shell, web tools, movement, conversation.
//! Phase 3 implementation.

pub mod shell;
pub mod web;
pub mod movement;
pub mod respond;

use anyhow::Result;
use std::path::Path;

/// Execute a tool by name. Returns the tool output string.
pub async fn execute_tool(
    name: &str,
    arguments: &serde_json::Value,
    env_root: &Path,
) -> Result<String> {
    match name {
        "shell" => {
            let command = arguments
                .get("command")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            Ok(shell::run_command(command, env_root))
        }
        "fetch_url" => {
            let url = arguments
                .get("url")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            web::fetch_url(url).await
        }
        "web_search" => {
            let query = arguments
                .get("query")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let max_results = arguments
                .get("max_results")
                .and_then(|v| v.as_u64())
                .unwrap_or(5) as usize;
            web::ollama_web_search(query, max_results, None).await
        }
        "web_fetch" => {
            let url = arguments
                .get("url")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            web::ollama_web_fetch(url, None).await
        }
        _ => Ok(format!("Unknown tool: {}", name)),
    }
}
