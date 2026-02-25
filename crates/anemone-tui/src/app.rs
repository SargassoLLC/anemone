//! App state, input handling, event loop.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use tokio::sync::RwLock;

use anemone_core::brain::{Brain, BrainCommand};
use anemone_core::config::Config;
use anemone_core::events::BrainEvent;
use anemone_core::identity;
use anemone_core::types::*;

/// A message in the chat feed.
#[derive(Clone)]
pub struct ChatMessage {
    pub side: ChatSide,
    pub text: String,
    pub phase: Phase,
}

#[derive(Clone, PartialEq)]
pub enum ChatSide {
    Left,   // system / user / tool results
    Right,  // anemone thoughts / tool calls
    System, // status
}

#[derive(Clone, PartialEq)]
pub enum Phase {
    Normal,
    Reflection,
    Planning,
}

/// Per-anemone state for the TUI.
pub struct AnemoneView {
    pub id: String,
    pub name: String,
    pub state: BrainState,
    pub thought_count: u32,
    pub position: Position,
    pub activity: String,
    pub messages: Vec<ChatMessage>,
    pub scroll_offset: usize,
    pub brain: Arc<RwLock<Brain>>,
    pub command_tx: tokio::sync::mpsc::Sender<BrainCommand>,
}

/// The main application state.
pub struct App {
    pub anemones: Vec<AnemoneView>,
    pub active_tab: usize,
    pub input: String,
    pub input_focused: bool,
    pub should_quit: bool,
}

impl App {
    /// Discover anemones and create the App.
    pub fn new(
        project_root: &Path,
        config: &Config,
    ) -> Self {
        let mut anemones = Vec::new();

        // Scan for *_box/ directories
        if let Ok(entries) = std::fs::read_dir(project_root) {
            let mut boxes: Vec<PathBuf> = entries
                .flatten()
                .filter_map(|e| {
                    let path = e.path();
                    if path.is_dir() {
                        if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                            if name.ends_with("_box") {
                                return Some(path);
                            }
                        }
                    }
                    None
                })
                .collect();
            boxes.sort();

            for box_path in boxes {
                if let Ok(Some(ident)) = identity::load_identity_from(&box_path) {
                    let anemone_id = box_path
                        .file_name()
                        .and_then(|n| n.to_str())
                        .and_then(|n| n.strip_suffix("_box"))
                        .unwrap_or("anemone")
                        .to_string();

                    let name = ident.name.clone();
                    let brain = Brain::new(ident, box_path, config.clone());
                    let command_tx = brain.command_sender();
                    let brain_arc = Arc::new(RwLock::new(brain));

                    anemones.push(AnemoneView {
                        id: anemone_id,
                        name,
                        state: BrainState::Idle,
                        thought_count: 0,
                        position: Position { x: 5, y: 5 },
                        activity: String::new(),
                        messages: Vec::new(),
                        scroll_offset: 0,
                        brain: brain_arc,
                        command_tx,
                    });
                }
            }
        }

        App {
            anemones,
            active_tab: 0,
            input: String::new(),
            input_focused: true,
            should_quit: false,
        }
    }

    pub fn active_view(&self) -> Option<&AnemoneView> {
        self.anemones.get(self.active_tab)
    }

    pub fn active_view_mut(&mut self) -> Option<&mut AnemoneView> {
        self.anemones.get_mut(self.active_tab)
    }

    /// Handle a brain event for a specific anemone.
    pub fn handle_event(&mut self, anemone_idx: usize, event: BrainEvent) {
        let Some(view) = self.anemones.get_mut(anemone_idx) else {
            return;
        };

        match event {
            BrainEvent::Entry(entry) => {
                let (side, phase) = match entry.event_type.as_str() {
                    "thought" => (ChatSide::Right, Phase::Normal),
                    "reflection" | "reflection_start" => (ChatSide::Right, Phase::Reflection),
                    "planning" => (ChatSide::Right, Phase::Planning),
                    "tool_call" => (ChatSide::Right, Phase::Normal),
                    "tool_result" => (ChatSide::Left, Phase::Normal),
                    "error" => (ChatSide::System, Phase::Normal),
                    _ => (ChatSide::System, Phase::Normal),
                };

                let text = entry
                    .data
                    .get("text")
                    .or_else(|| entry.data.get("command"))
                    .or_else(|| entry.data.get("content"))
                    .or_else(|| entry.data.get("output"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();

                if !text.is_empty() {
                    let prefix = match entry.event_type.as_str() {
                        "tool_call" => {
                            let tool = entry
                                .data
                                .get("tool")
                                .and_then(|v| v.as_str())
                                .unwrap_or("?");
                            format!("[{}] ", tool)
                        }
                        "tool_result" => {
                            let tool = entry
                                .data
                                .get("tool")
                                .and_then(|v| v.as_str())
                                .unwrap_or("?");
                            format!("[{} result] ", tool)
                        }
                        _ => String::new(),
                    };
                    view.messages.push(ChatMessage {
                        side,
                        text: format!("{}{}", prefix, text),
                        phase,
                    });
                    // Auto-scroll to bottom
                    view.scroll_offset = 0;
                }
            }
            BrainEvent::Status(status) => {
                view.state = status.state;
                view.thought_count = status.thought_count;
            }
            BrainEvent::Position(pos) => {
                view.position = pos;
            }
            BrainEvent::Activity(activity) => {
                view.activity = if activity.activity_type == "idle" {
                    String::new()
                } else {
                    activity.detail
                };
            }
            BrainEvent::Alert => {
                view.messages.push(ChatMessage {
                    side: ChatSide::System,
                    text: "New file detected!".to_string(),
                    phase: Phase::Normal,
                });
            }
            BrainEvent::Conversation(conv) => {
                if let Some(msg) = conv.message {
                    view.messages.push(ChatMessage {
                        side: ChatSide::Right,
                        text: msg,
                        phase: Phase::Normal,
                    });
                }
            }
            BrainEvent::FocusMode(fm) => {
                view.messages.push(ChatMessage {
                    side: ChatSide::System,
                    text: format!(
                        "Focus mode {}",
                        if fm.enabled { "ON" } else { "OFF" }
                    ),
                    phase: Phase::Normal,
                });
            }
            _ => {}
        }
    }

    /// Send a user message to the active anemone.
    pub async fn send_message(&mut self) {
        if self.input.trim().is_empty() {
            return;
        }
        let text = self.input.clone();
        self.input.clear();

        if let Some(view) = self.anemones.get_mut(self.active_tab) {
            let cmd = if text.starts_with("/focus") {
                let enabled = !text.contains("off");
                BrainCommand::SetFocusMode(enabled)
            } else {
                // Show user message in chat
                view.messages.push(ChatMessage {
                    side: ChatSide::Left,
                    text: format!("You: {}", text),
                    phase: Phase::Normal,
                });
                view.scroll_offset = 0;

                let brain = view.brain.read().await;
                if brain.is_waiting_for_reply() {
                    BrainCommand::ConversationReply(text)
                } else {
                    BrainCommand::UserMessage(text)
                }
            };
            let _ = view.command_tx.send(cmd).await;
        }
    }

    pub fn next_tab(&mut self) {
        if !self.anemones.is_empty() {
            self.active_tab = (self.active_tab + 1) % self.anemones.len();
        }
    }

    pub fn prev_tab(&mut self) {
        if !self.anemones.is_empty() {
            if self.active_tab == 0 {
                self.active_tab = self.anemones.len() - 1;
            } else {
                self.active_tab -= 1;
            }
        }
    }

    pub fn scroll_up(&mut self) {
        if let Some(view) = self.active_view_mut() {
            view.scroll_offset = view.scroll_offset.saturating_add(3);
        }
    }

    pub fn scroll_down(&mut self) {
        if let Some(view) = self.active_view_mut() {
            view.scroll_offset = view.scroll_offset.saturating_sub(3);
        }
    }
}
