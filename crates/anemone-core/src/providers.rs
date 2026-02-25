//! LLM provider routing — Responses API (OpenAI) or Chat Completions (everything else).
//! 1:1 port of Python providers.py using reqwest for HTTP calls.

use anyhow::{Context, Result};
use serde_json::json;
use tracing::{error, info};

use crate::config::Config;
use crate::types::{LlmResponse, ToolCall};

/// Max chars of tool result content sent to the model.
const MAX_TOOL_CONTENT: usize = 16000;

// ── Tool definitions (1:1 with Python TOOLS) ──

pub fn tool_definitions(config: &Config) -> Vec<serde_json::Value> {
    let mut tools = vec![
        json!({
            "type": "function",
            "name": "shell",
            "description": "Run a shell command inside your environment folder. You can use ls, cat, mkdir, mv, cp, touch, echo, tee, find, grep, head, tail, wc, etc. You can also run Python scripts: 'python script.py' or 'python -c \"code\"'. Use 'cat > file.txt << EOF' or 'echo ... > file.txt' to write files. Create folders with mkdir. Organize however you like. All paths are relative to your environment root.",
            "parameters": {
                "type": "object",
                "properties": {
                    "command": { "type": "string", "description": "The shell command to run" }
                },
                "required": ["command"]
            }
        }),
        json!({
            "type": "function",
            "name": "respond",
            "description": "Talk to your owner! Use this whenever you hear their voice and want to reply. After you speak, they might say something back — if they do, use respond AGAIN to keep the conversation going. You can go back and forth as many times as you like.",
            "parameters": {
                "type": "object",
                "properties": {
                    "message": { "type": "string", "description": "What you say back to them" }
                },
                "required": ["message"]
            }
        }),
        json!({
            "type": "function",
            "name": "fetch_url",
            "description": "Fetch the content of a web page. Use this for research when you need to read an article, documentation, or any URL. Returns the page content (HTML or text). Only http and https URLs are allowed.",
            "parameters": {
                "type": "object",
                "properties": {
                    "url": { "type": "string", "description": "The URL to fetch (must start with http:// or https://)" }
                },
                "required": ["url"]
            }
        }),
        json!({
            "type": "function",
            "name": "move",
            "description": "Move to a location in your room. Use this to go where feels natural for what you're doing — desk for writing, bookshelf for research, window for pondering, bed for resting.",
            "parameters": {
                "type": "object",
                "properties": {
                    "location": {
                        "type": "string",
                        "enum": ["desk", "bookshelf", "window", "plant", "bed", "rug", "center"]
                    }
                },
                "required": ["location"]
            }
        }),
    ];

    // Ollama cloud web search tools
    if config.ollama_api_key.is_some() && config.provider == "custom" {
        tools.push(json!({
            "type": "function",
            "name": "web_search",
            "description": "Search the web for current information. Use for research, fact-checking, or finding recent news. Returns titles, URLs, and content snippets.",
            "parameters": {
                "type": "object",
                "properties": {
                    "query": { "type": "string", "description": "Search query" },
                    "max_results": { "type": "integer", "description": "Max results to return (default 5, max 10)" }
                },
                "required": ["query"]
            }
        }));
        tools.push(json!({
            "type": "function",
            "name": "web_fetch",
            "description": "Fetch the full content of a specific URL. Use after web_search to read a page in detail. Returns page title, content, and links.",
            "parameters": {
                "type": "object",
                "properties": {
                    "url": { "type": "string", "description": "URL to fetch (e.g. https://...)" }
                },
                "required": ["url"]
            }
        }));
    }

    tools
}

/// Convert Responses API tool defs to Chat Completions format.
/// Drops non-function tools. Wraps in {"type": "function", "function": {...}}.
pub fn tools_for_completions(tools: &[serde_json::Value]) -> Vec<serde_json::Value> {
    tools
        .iter()
        .filter(|t| t.get("type").and_then(|v| v.as_str()) == Some("function"))
        .map(|t| {
            json!({
                "type": "function",
                "function": {
                    "name": t["name"],
                    "description": t.get("description").unwrap_or(&json!("")),
                    "parameters": t.get("parameters").unwrap_or(&json!({})),
                }
            })
        })
        .collect()
}

// ── Input translation ──

/// Convert multimodal content from Responses API to Chat Completions format.
fn translate_multimodal(content_parts: &[serde_json::Value]) -> Vec<serde_json::Value> {
    content_parts
        .iter()
        .filter_map(|part| {
            let part_type = part.get("type")?.as_str()?;
            match part_type {
                "input_image" => Some(json!({
                    "type": "image_url",
                    "image_url": { "url": part["image_url"] }
                })),
                "input_text" => Some(json!({
                    "type": "text",
                    "text": part["text"]
                })),
                _ => Some(part.clone()),
            }
        })
        .collect()
}

/// Convert Responses API input_list to Chat Completions messages.
fn translate_input_to_messages(
    input_list: &[serde_json::Value],
    instructions: Option<&str>,
) -> Vec<serde_json::Value> {
    let mut messages = Vec::new();

    if let Some(inst) = instructions {
        messages.push(json!({"role": "system", "content": inst}));
    }

    for item in input_list {
        if item.get("type").and_then(|v| v.as_str()) == Some("function_call_output") {
            let mut content = item
                .get("output")
                .map(|v| v.to_string())
                .unwrap_or_default();
            // Strip quotes if it's a JSON string
            if content.starts_with('"') && content.ends_with('"') {
                content = content[1..content.len() - 1].to_string();
            }
            if content.len() > MAX_TOOL_CONTENT {
                content.truncate(MAX_TOOL_CONTENT);
                content.push_str("\n...(truncated)");
            }
            let mut tool_msg = json!({
                "role": "tool",
                "content": content,
            });
            if let Some(call_id) = item.get("call_id") {
                tool_msg["tool_call_id"] = call_id.clone();
            }
            messages.push(tool_msg);
        } else if item.get("role").is_some() {
            let mut msg = item.clone();
            // Translate multimodal content
            if let Some(content) = item.get("content") {
                if content.is_array() {
                    let parts: Vec<serde_json::Value> =
                        serde_json::from_value(content.clone()).unwrap_or_default();
                    msg["content"] = json!(translate_multimodal(&parts));
                }
            }
            messages.push(msg);
        }
        // Skip non-dict / SDK objects (they don't exist in our Rust representation)
    }

    messages
}

// ── Response normalization ──

/// Normalize a Chat Completions response into our standard format.
fn normalize_completions_response(response: &serde_json::Value) -> LlmResponse {
    let message = &response["choices"][0]["message"];
    let text = message.get("content").and_then(|v| v.as_str()).map(String::from);

    let mut tool_calls = Vec::new();
    let mut output = Vec::new();

    if let Some(tcs) = message.get("tool_calls").and_then(|v| v.as_array()) {
        for (i, tc) in tcs.iter().enumerate() {
            let func = &tc["function"];
            let call_id = tc
                .get("id")
                .and_then(|v| v.as_str())
                .map(String::from)
                .unwrap_or_else(|| {
                    format!(
                        "call_{}_{}",
                        func.get("name").and_then(|v| v.as_str()).unwrap_or("unknown"),
                        i
                    )
                });

            let name = func
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();

            let arguments: serde_json::Value = func
                .get("arguments")
                .and_then(|v| v.as_str())
                .and_then(|s| serde_json::from_str(s).ok())
                .unwrap_or(json!({}));

            tool_calls.push(ToolCall {
                name: name.clone(),
                arguments,
                call_id: call_id.clone(),
            });
        }

        // Build synthetic assistant message for follow-up input_list
        let tc_output: Vec<serde_json::Value> = tcs
            .iter()
            .enumerate()
            .map(|(i, tc)| {
                let call_id = tc
                    .get("id")
                    .and_then(|v| v.as_str())
                    .map(String::from)
                    .unwrap_or_else(|| {
                        format!(
                            "call_{}_{}",
                            tc["function"]
                                .get("name")
                                .and_then(|v| v.as_str())
                                .unwrap_or("unknown"),
                            i
                        )
                    });
                json!({
                    "id": call_id,
                    "type": "function",
                    "function": {
                        "name": tc["function"]["name"],
                        "arguments": tc["function"]["arguments"],
                    }
                })
            })
            .collect();

        output.push(json!({
            "role": "assistant",
            "content": text,
            "tool_calls": tc_output,
        }));
    }

    LlmResponse {
        text,
        tool_calls,
        output,
    }
}

// ── API calls ──

fn build_client() -> reqwest::Client {
    reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(60))
        .build()
        .expect("Failed to build HTTP client")
}

/// Make a Chat Completions API call.
async fn chat_completions(
    config: &Config,
    input_list: &[serde_json::Value],
    use_tools: bool,
    instructions: Option<&str>,
    max_tokens: u32,
) -> Result<LlmResponse> {
    let messages = translate_input_to_messages(input_list, instructions);

    let api_key = config
        .api_key
        .as_deref()
        .unwrap_or("ollama"); // Ollama doesn't need a key

    let base_url = config
        .base_url
        .as_deref()
        .unwrap_or("https://api.openai.com/v1");

    let url = format!("{}/chat/completions", base_url.trim_end_matches('/'));

    let mut body = json!({
        "model": config.model,
        "messages": messages,
        "max_tokens": max_tokens,
    });

    if use_tools {
        let tools = tool_definitions(config);
        let completions_tools = tools_for_completions(&tools);
        if !completions_tools.is_empty() {
            body["tools"] = json!(completions_tools);
        }
    }

    info!(
        "chat_completions request: model={} provider={} msg_count={}",
        config.model,
        config.provider,
        messages.len()
    );

    let client = build_client();
    let response = client
        .post(&url)
        .header("Authorization", format!("Bearer {}", api_key))
        .header("Content-Type", "application/json")
        .json(&body)
        .send()
        .await
        .context("HTTP request failed")?;

    let status = response.status();
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        error!("API HTTP {}: {} | url={}", status, &body[..body.len().min(500)], url);

        // Retry once on 500 errors (transient Ollama issues)
        if status.as_u16() == 500 {
            tokio::time::sleep(std::time::Duration::from_secs(2)).await;
            let retry_response = client
                .post(&url)
                .header("Authorization", format!("Bearer {}", api_key))
                .header("Content-Type", "application/json")
                .json(&body)
                .send()
                .await
                .context("Retry HTTP request failed")?;

            if !retry_response.status().is_success() {
                anyhow::bail!(
                    "API call failed after retry: HTTP {}",
                    retry_response.status()
                );
            }

            let data: serde_json::Value = retry_response
                .json()
                .await
                .context("Failed to parse retry response")?;
            return Ok(normalize_completions_response(&data));
        }

        anyhow::bail!("API call failed: HTTP {} — {}", status, &body[..body.len().min(200)]);
    }

    let data: serde_json::Value = response
        .json()
        .await
        .context("Failed to parse API response")?;

    Ok(normalize_completions_response(&data))
}

/// Make an OpenAI Responses API call.
async fn chat_responses(
    config: &Config,
    input_list: &[serde_json::Value],
    use_tools: bool,
    instructions: Option<&str>,
    max_tokens: u32,
) -> Result<LlmResponse> {
    let api_key = config
        .api_key
        .as_deref()
        .context("API key required for OpenAI")?;

    let url = "https://api.openai.com/v1/responses";

    let mut body = json!({
        "model": config.model,
        "input": input_list,
        "max_output_tokens": max_tokens,
    });

    if let Some(inst) = instructions {
        body["instructions"] = json!(inst);
    }

    if use_tools {
        let tools = tool_definitions(config);
        body["tools"] = json!(tools);
    }

    let client = build_client();
    let response = client
        .post(url)
        .header("Authorization", format!("Bearer {}", api_key))
        .header("Content-Type", "application/json")
        .json(&body)
        .send()
        .await
        .context("HTTP request failed")?;

    let status = response.status();
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        anyhow::bail!("OpenAI API HTTP {} — {}", status, &body[..body.len().min(500)]);
    }

    let data: serde_json::Value = response
        .json()
        .await
        .context("Failed to parse API response")?;

    // Parse Responses API output
    let output = data
        .get("output")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();

    let mut text_parts = Vec::new();
    let mut tool_calls = Vec::new();

    for item in &output {
        let item_type = item.get("type").and_then(|v| v.as_str()).unwrap_or("");
        match item_type {
            "message" => {
                if let Some(content) = item.get("content").and_then(|v| v.as_array()) {
                    for c in content {
                        if let Some(text) = c.get("text").and_then(|v| v.as_str()) {
                            text_parts.push(text.to_string());
                        }
                    }
                }
            }
            "function_call" => {
                let name = item
                    .get("name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let arguments: serde_json::Value = item
                    .get("arguments")
                    .and_then(|v| v.as_str())
                    .and_then(|s| serde_json::from_str(s).ok())
                    .unwrap_or(json!({}));
                let call_id = item
                    .get("call_id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();

                tool_calls.push(ToolCall {
                    name,
                    arguments,
                    call_id,
                });
            }
            _ => {}
        }
    }

    let text = if text_parts.is_empty() {
        None
    } else {
        Some(text_parts.join("\n"))
    };

    Ok(LlmResponse {
        text,
        tool_calls,
        output,
    })
}

// ── Public API ──

fn uses_responses_api(config: &Config) -> bool {
    config.provider == "openai"
}

/// Make an LLM call. Routes to Responses API or Chat Completions based on provider.
pub async fn chat(
    config: &Config,
    input: &[serde_json::Value],
    tools: bool,
    instructions: Option<&str>,
    max_tokens: u32,
) -> Result<LlmResponse> {
    if uses_responses_api(config) {
        chat_responses(config, input, tools, instructions, max_tokens).await
    } else {
        chat_completions(config, input, tools, instructions, max_tokens).await
    }
}

/// Short LLM call (for importance scoring, reflections) — just returns text, no tools.
pub async fn chat_short(
    config: &Config,
    input: &[serde_json::Value],
    instructions: Option<&str>,
) -> Result<String> {
    let result = chat(config, input, false, instructions, 300).await?;
    Ok(result.text.unwrap_or_default())
}

/// Get an embedding vector for a text string.
pub async fn embed(config: &Config, text: &str) -> Result<Vec<f64>> {
    let model = &config.embedding_model;

    let (url, api_key) = if uses_responses_api(config) {
        (
            "https://api.openai.com/v1/embeddings".to_string(),
            config.api_key.clone().context("API key required for embeddings")?,
        )
    } else if let Some(ref base) = config.base_url {
        let key = config.api_key.clone().unwrap_or_else(|| "ollama".to_string());
        (
            format!("{}/embeddings", base.trim_end_matches('/')),
            key,
        )
    } else {
        // Fallback to OpenAI for embeddings
        let key = std::env::var("OPENAI_API_KEY")
            .context("OPENAI_API_KEY required for embeddings fallback")?;
        ("https://api.openai.com/v1/embeddings".to_string(), key)
    };

    let body = json!({
        "model": model,
        "input": text,
    });

    let client = build_client();
    let response = client
        .post(&url)
        .header("Authorization", format!("Bearer {}", api_key))
        .header("Content-Type", "application/json")
        .json(&body)
        .send()
        .await
        .context("Embedding request failed")?;

    let status = response.status();
    if !status.is_success() {
        // If non-OpenAI provider fails, try OpenAI fallback
        if !uses_responses_api(config) {
            if let Ok(fallback_key) = std::env::var("OPENAI_API_KEY") {
                let fallback_resp = client
                    .post("https://api.openai.com/v1/embeddings")
                    .header("Authorization", format!("Bearer {}", fallback_key))
                    .header("Content-Type", "application/json")
                    .json(&body)
                    .send()
                    .await?;

                if fallback_resp.status().is_success() {
                    let data: serde_json::Value = fallback_resp.json().await?;
                    let embedding = data["data"][0]["embedding"]
                        .as_array()
                        .context("Invalid embedding response")?
                        .iter()
                        .filter_map(|v| v.as_f64())
                        .collect();
                    return Ok(embedding);
                }
            }
        }
        let body_text = response.text().await.unwrap_or_default();
        anyhow::bail!("Embedding API HTTP {} — {}", status, &body_text[..body_text.len().min(200)]);
    }

    let data: serde_json::Value = response.json().await?;
    let embedding = data["data"][0]["embedding"]
        .as_array()
        .context("Invalid embedding response")?
        .iter()
        .filter_map(|v| v.as_f64())
        .collect();

    Ok(embedding)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tool_definitions_basic() {
        let config = Config::default();
        let tools = tool_definitions(&config);
        assert!(tools.len() >= 4); // shell, respond, fetch_url, move

        let names: Vec<&str> = tools
            .iter()
            .filter_map(|t| t.get("name").and_then(|v| v.as_str()))
            .collect();
        assert!(names.contains(&"shell"));
        assert!(names.contains(&"respond"));
        assert!(names.contains(&"fetch_url"));
        assert!(names.contains(&"move"));
    }

    #[test]
    fn test_tools_for_completions() {
        let config = Config::default();
        let tools = tool_definitions(&config);
        let completions = tools_for_completions(&tools);

        // All should have type=function and a function object
        for t in &completions {
            assert_eq!(t["type"].as_str(), Some("function"));
            assert!(t.get("function").is_some());
            assert!(t["function"].get("name").is_some());
        }
    }

    #[test]
    fn test_translate_input_to_messages() {
        let input = vec![
            json!({"role": "user", "content": "Hello"}),
            json!({"type": "function_call_output", "call_id": "c1", "output": "result here"}),
        ];

        let messages = translate_input_to_messages(&input, Some("You are helpful"));

        assert_eq!(messages.len(), 3); // system + user + tool
        assert_eq!(messages[0]["role"], "system");
        assert_eq!(messages[1]["role"], "user");
        assert_eq!(messages[2]["role"], "tool");
    }

    #[test]
    fn test_normalize_completions_response_text_only() {
        let response = json!({
            "choices": [{
                "message": {
                    "content": "Hello world",
                    "role": "assistant"
                }
            }]
        });

        let result = normalize_completions_response(&response);
        assert_eq!(result.text.as_deref(), Some("Hello world"));
        assert!(result.tool_calls.is_empty());
    }

    #[test]
    fn test_normalize_completions_response_with_tools() {
        let response = json!({
            "choices": [{
                "message": {
                    "content": "Let me check.",
                    "role": "assistant",
                    "tool_calls": [{
                        "id": "call_123",
                        "type": "function",
                        "function": {
                            "name": "shell",
                            "arguments": "{\"command\": \"ls\"}"
                        }
                    }]
                }
            }]
        });

        let result = normalize_completions_response(&response);
        assert_eq!(result.text.as_deref(), Some("Let me check."));
        assert_eq!(result.tool_calls.len(), 1);
        assert_eq!(result.tool_calls[0].name, "shell");
        assert_eq!(result.tool_calls[0].call_id, "call_123");
        assert_eq!(result.tool_calls[0].arguments["command"], "ls");
    }
}
