//! Anemone WASM frontend — Dioxus app root.

mod api;
mod components;
mod ws;

use dioxus::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AnemoneInfo {
    pub id: String,
    pub name: String,
    pub state: String,
    pub thought_count: u32,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Position {
    pub x: i32,
    pub y: i32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ChatMsg {
    pub side: String,     // "left", "right", "system"
    pub text: String,
    pub phase: String,    // "normal", "reflection", "planning"
}

fn main() {
    dioxus::launch(App);
}

fn App() -> Element {
    let mut anemones = use_signal(|| Vec::<AnemoneInfo>::new());
    let mut active_id = use_signal(|| String::new());
    let mut position = use_signal(|| Position { x: 5, y: 5 });
    let mut state = use_signal(|| "idle".to_string());
    let mut activity = use_signal(|| String::new());
    let mut messages = use_signal(|| Vec::<ChatMsg>::new());
    let mut conversing = use_signal(|| false);
    let mut countdown = use_signal(|| 0u32);
    let mut name = use_signal(|| "anemone".to_string());

    // Fetch anemones list on mount
    use_effect(move || {
        spawn(async move {
            if let Ok(list) = api::fetch_anemones().await {
                if !list.is_empty() {
                    let first = list[0].clone();
                    active_id.set(first.id.clone());
                    name.set(first.name.clone());

                    // Load historical state
                    if let Ok(status) = api::fetch_status(&first.id).await {
                        if let Some(pos) = status.get("position") {
                            if let Ok(p) = serde_json::from_value::<Position>(pos.clone()) {
                                position.set(p);
                            }
                        }
                        if let Some(s) = status.get("state").and_then(|v| v.as_str()) {
                            state.set(s.to_string());
                        }
                    }
                }
                anemones.set(list);
            }
        });
    });

    // Connect WebSocket when active_id changes
    let ws_active_id = active_id();
    use_effect(move || {
        let id = ws_active_id.clone();
        if id.is_empty() {
            return;
        }
        spawn(async move {
            ws::connect_ws(
                &id,
                move |event| {
                    match event.get("event").and_then(|v| v.as_str()) {
                        Some("position") => {
                            if let Some(data) = event.get("data") {
                                if let Ok(p) = serde_json::from_value::<Position>(data.clone()) {
                                    position.set(p);
                                }
                            }
                        }
                        Some("status") => {
                            if let Some(data) = event.get("data") {
                                if let Some(s) = data.get("state").and_then(|v| v.as_str()) {
                                    state.set(s.to_string());
                                }
                            }
                        }
                        Some("activity") => {
                            if let Some(data) = event.get("data") {
                                let detail = data
                                    .get("detail")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("")
                                    .to_string();
                                activity.set(detail);
                            }
                        }
                        Some("entry") => {
                            if let Some(data) = event.get("data") {
                                let event_type = data
                                    .get("event_type")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("");
                                let text = data
                                    .get("data")
                                    .and_then(|d| {
                                        d.get("text")
                                            .or_else(|| d.get("output"))
                                            .or_else(|| d.get("command"))
                                    })
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("");

                                if !text.is_empty() {
                                    let (side, phase) = match event_type {
                                        "thought" => ("right", "normal"),
                                        "reflection" => ("right", "reflection"),
                                        "planning" => ("right", "planning"),
                                        "tool_call" => ("right", "normal"),
                                        "tool_result" => ("left", "normal"),
                                        "error" => ("system", "normal"),
                                        _ => ("system", "normal"),
                                    };
                                    let prefix = match event_type {
                                        "tool_call" => {
                                            let tool = data
                                                .get("data")
                                                .and_then(|d| d.get("tool"))
                                                .and_then(|v| v.as_str())
                                                .unwrap_or("?");
                                            format!("[{}] ", tool)
                                        }
                                        _ => String::new(),
                                    };
                                    messages.push(ChatMsg {
                                        side: side.to_string(),
                                        text: format!("{}{}", prefix, text),
                                        phase: phase.to_string(),
                                    });
                                }
                            }
                        }
                        Some("conversation") => {
                            if let Some(data) = event.get("data") {
                                let conv_state =
                                    data.get("state").and_then(|v| v.as_str()).unwrap_or("");
                                if conv_state == "waiting" {
                                    conversing.set(true);
                                    let timeout =
                                        data.get("timeout").and_then(|v| v.as_u64()).unwrap_or(15);
                                    countdown.set(timeout as u32);
                                    if let Some(msg) =
                                        data.get("message").and_then(|v| v.as_str())
                                    {
                                        messages.push(ChatMsg {
                                            side: "right".to_string(),
                                            text: msg.to_string(),
                                            phase: "normal".to_string(),
                                        });
                                    }
                                } else if conv_state == "ended" {
                                    conversing.set(false);
                                    countdown.set(0);
                                }
                            }
                        }
                        Some("alert") => {
                            messages.push(ChatMsg {
                                side: "system".to_string(),
                                text: "New file detected!".to_string(),
                                phase: "normal".to_string(),
                            });
                        }
                        _ => {}
                    }
                },
            )
            .await;
        });
    });

    rsx! {
        div { id: "main",
            // Switcher
            components::switcher::Switcher {
                anemones: anemones(),
                active_id: active_id(),
                on_switch: move |id: String| {
                    active_id.set(id.clone());
                    messages.set(Vec::new());
                    let a = anemones().iter().find(|a| a.id == id).cloned();
                    if let Some(a) = a {
                        name.set(a.name);
                    }
                },
            }

            // Content
            div { class: "content",
                // Game panel
                components::game_world::GameWorld {
                    position: position(),
                    state: state(),
                    activity: activity(),
                    name: name(),
                }

                // Chat panel
                div { class: "chat-panel",
                    components::chat_feed::ChatFeed {
                        messages: messages(),
                    }

                    components::input_bar::InputBar {
                        active_id: active_id(),
                        conversing: conversing(),
                        countdown: countdown(),
                    }
                }
            }
        }
    }
}
