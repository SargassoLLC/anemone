//! App state, input handling, event loop.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use crossterm::event::KeyCode;
use tokio::sync::RwLock;

use anemone_core::brain::{Brain, BrainCommand};
use anemone_core::config::Config;
use anemone_core::events::BrainEvent;
use anemone_core::identity;
use anemone_core::types::*;

use crate::ui::setup::{SetupState, SetupStep};

// ─── AppMode ──────────────────────────────────────────────────────────────────

/// Top-level application mode — controls which screen is rendered.
#[derive(Debug, Clone, PartialEq)]
pub enum AppMode {
    /// First-run setup wizard: guides user through provider + API key config.
    Setup,
    /// Normal TUI: anemone room, chat feed, input bar.
    Running,
}

// ─── Chat / view types ────────────────────────────────────────────────────────

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

// ─── App ─────────────────────────────────────────────────────────────────────

/// The main application state.
pub struct App {
    // ── Mode ────────────────────────────────────────────────────────────────
    /// Current top-level application mode.
    pub mode: AppMode,

    /// Setup wizard state — `Some` while in Setup mode, `None` otherwise.
    pub setup_state: Option<SetupState>,

    // ── Running-mode state ──────────────────────────────────────────────────
    pub anemones: Vec<AnemoneView>,
    pub active_tab: usize,
    pub input: String,
    pub input_focused: bool,
    pub should_quit: bool,
}

impl App {
    /// Discover anemones and create the App in the given initial mode.
    ///
    /// When `initial_mode` is [`AppMode::Setup`] the anemone list is **not**
    /// populated yet — `finish_setup` must be called once setup is complete.
    pub fn new(project_root: &Path, config: &Config, initial_mode: AppMode) -> Self {
        let setup_state = if initial_mode == AppMode::Setup {
            Some(SetupState::new())
        } else {
            None
        };

        let anemones = if initial_mode == AppMode::Running {
            Self::discover_anemones(project_root, config)
        } else {
            Vec::new()
        };

        App {
            mode: initial_mode,
            setup_state,
            anemones,
            active_tab: 0,
            input: String::new(),
            input_focused: true,
            should_quit: false,
        }
    }

    // ── Setup helpers ─────────────────────────────────────────────────────────

    /// Handle a key event while in Setup mode.
    ///
    /// Returns `true` when setup has completed (caller should save config and
    /// transition to `AppMode::Running`).
    pub fn handle_setup_key(&mut self, key: KeyCode) -> bool {
        let Some(state) = self.setup_state.as_mut() else {
            return false;
        };

        match &state.step {
            // ── Provider selection ──────────────────────────────────────────
            SetupStep::ProviderSelect => {
                let count = state.providers.len();
                match key {
                    KeyCode::Up => {
                        if state.selected_provider > 0 {
                            state.selected_provider -= 1;
                        } else {
                            state.selected_provider = count - 1;
                        }
                    }
                    KeyCode::Down => {
                        state.selected_provider = (state.selected_provider + 1) % count;
                    }
                    // Number shortcuts: 1–4
                    KeyCode::Char('1') => state.selected_provider = 0,
                    KeyCode::Char('2') => state.selected_provider = 1,
                    KeyCode::Char('3') => state.selected_provider = 2,
                    KeyCode::Char('4') => state.selected_provider = 3,
                    KeyCode::Enter => {
                        // Advance to the appropriate next step
                        if let Some(provider) = state.providers.get(state.selected_provider) {
                            if provider.needs_key {
                                state.step = SetupStep::ApiKeyInput;
                            } else if provider.needs_url {
                                state.step = SetupStep::CustomUrlInput;
                            } else {
                                state.step = SetupStep::NameInput;
                            }
                        }
                    }
                    _ => {}
                }
            }

            // ── API key input ────────────────────────────────────────────────
            SetupStep::ApiKeyInput => {
                match key {
                    KeyCode::Char(c) => {
                        state.key_input.push(c);
                    }
                    KeyCode::Backspace => {
                        state.key_input.pop();
                    }
                    KeyCode::Enter => {
                        // Only advance when we have something
                        if !state.key_input.is_empty() {
                            // Check whether we also need a URL
                            if let Some(provider) = state.providers.get(state.selected_provider) {
                                if provider.needs_url {
                                    state.step = SetupStep::CustomUrlInput;
                                } else {
                                    state.step = SetupStep::NameInput;
                                }
                            } else {
                                state.step = SetupStep::NameInput;
                            }
                        }
                    }
                    KeyCode::Esc => {
                        // Back to provider selection
                        state.step = SetupStep::ProviderSelect;
                    }
                    _ => {}
                }
            }

            // ── Custom URL input ─────────────────────────────────────────────
            SetupStep::CustomUrlInput => {
                match key {
                    KeyCode::Char(c) => {
                        state.url_input.push(c);
                    }
                    KeyCode::Backspace => {
                        state.url_input.pop();
                    }
                    KeyCode::Enter => {
                        // Accept any non-empty URL (or the default for Ollama)
                        let url = state.url_input.trim().to_string();
                        if url.is_empty() {
                            // Default to localhost Ollama
                            state.url_input = "http://localhost:11434".to_string();
                        }
                        state.step = SetupStep::NameInput;
                    }
                    KeyCode::Esc => {
                        // Back: if key was needed go back to key input, else provider select
                        if let Some(provider) = state.providers.get(state.selected_provider) {
                            if provider.needs_key {
                                state.step = SetupStep::ApiKeyInput;
                            } else {
                                state.step = SetupStep::ProviderSelect;
                            }
                        }
                    }
                    _ => {}
                }
            }

            // ── Name input ───────────────────────────────────────────────────
            SetupStep::NameInput => {
                match key {
                    KeyCode::Char(c) => {
                        state.name_input.push(c);
                    }
                    KeyCode::Backspace => {
                        state.name_input.pop();
                    }
                    KeyCode::Enter => {
                        if state.name_input.trim().is_empty() {
                            state.name_input = "coral".to_string();
                        }
                        state.step = SetupStep::EntropyMash;
                    }
                    KeyCode::Esc => {
                        if let Some(provider) = state.providers.get(state.selected_provider) {
                            if provider.needs_url {
                                state.step = SetupStep::CustomUrlInput;
                            } else if provider.needs_key {
                                state.step = SetupStep::ApiKeyInput;
                            } else {
                                state.step = SetupStep::ProviderSelect;
                            }
                        }
                    }
                    _ => {}
                }
            }

            // ── Entropy mash ─────────────────────────────────────────────────
            SetupStep::EntropyMash => {
                match key {
                    KeyCode::Char(c) => {
                        state.entropy_input.push(c);
                        state.entropy_count += 1;
                    }
                    KeyCode::Enter => {
                        if state.entropy_count >= 20 {
                            state.step = SetupStep::Complete;
                        }
                    }
                    KeyCode::Esc => {
                        state.step = SetupStep::NameInput;
                    }
                    _ => {
                        state.entropy_input.push('?');
                        state.entropy_count += 1;
                    }
                }
            }

            // ── Setup complete — Enter launches TUI ──────────────────────────
            SetupStep::Complete => {
                if key == KeyCode::Enter {
                    return true; // Signal caller to finalize and transition
                }
            }
        }

        false
    }

    /// Consume the setup state, build a `Config` patch, and transition to
    /// `AppMode::Running`.  Returns the modified `Config` so the caller can
    /// persist it via `config.save(path)`.
    pub fn finish_setup(&mut self, project_root: &Path, config: &Config) -> Config {
        let mut new_config = config.clone();

        if let Some(ref state) = self.setup_state {
            if let Some(provider) = state.providers.get(state.selected_provider) {
                new_config.provider = provider.provider_id.clone();
                new_config.model = provider.default_model.clone();
            }
            if !state.key_input.is_empty() {
                new_config.api_key = Some(state.key_input.clone());
            }
            if !state.url_input.is_empty() {
                new_config.base_url = Some(state.url_input.clone());
            }

            // Create the first anemone identity
            let name = if state.name_input.trim().is_empty() {
                "coral".to_string()
            } else {
                state.name_input.trim().to_string()
            };

            let seed_bytes = if state.entropy_input.is_empty() {
                // Fallback: random seed
                use rand::RngCore;
                let mut bytes = [0u8; 32];
                rand::thread_rng().fill_bytes(&mut bytes);
                bytes.to_vec()
            } else {
                use sha2::{Sha256, Digest};
                let mut hasher = Sha256::new();
                hasher.update(state.entropy_input.as_bytes());
                hasher.finalize().to_vec()
            };

            let ident = identity::create_identity(&name, &seed_bytes);
            let box_path = project_root.join(format!("{}_box", name.to_lowercase()));
            if let Err(e) = identity::save_identity(&ident, &box_path) {
                tracing::warn!("Failed to save identity: {e}");
            }
        }

        // Populate anemones now that we have a working config
        self.anemones = Self::discover_anemones(project_root, &new_config);
        self.setup_state = None;
        self.mode = AppMode::Running;

        new_config
    }

    // ── Internal helpers ──────────────────────────────────────────────────────

    fn discover_anemones(project_root: &Path, config: &Config) -> Vec<AnemoneView> {
        let mut anemones = Vec::new();

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

        anemones
    }

    // ── Running-mode helpers ──────────────────────────────────────────────────

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
