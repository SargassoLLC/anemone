//! The thinking loop — the heart of the anemone. 1:1 port of Python brain.py.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use serde_json::json;
use tokio::sync::{broadcast, mpsc};
use tracing::{error, info, warn};

use crate::config::Config;
use crate::events::BrainEvent;
use crate::memory::MemoryStream;
use crate::prompts::{
    main_system_prompt, FOCUS_NUDGE, PLANNING_PROMPT, REFLECTION_PROMPT,
};
use crate::providers;
use crate::tools;
use crate::tools::shell::{IGNORE_FILES, INTERNAL_ROOT_FILES};
use crate::types::*;

/// Planning frequency — plan every N think cycles
pub const PLAN_INTERVAL: u32 = 10;

/// Messages that can be sent TO the brain (from API/TUI)
#[derive(Debug)]
pub enum BrainCommand {
    UserMessage(String),
    ConversationReply(String),
    SetFocusMode(bool),
    Snapshot(String),
    Stop,
}

/// The Brain — runs as an independent tokio task.
pub struct Brain {
    pub identity: Identity,
    pub env_path: PathBuf,
    pub events: Vec<EventEntry>,
    pub api_calls: Vec<ApiCallRecord>,
    pub thought_count: u32,
    pub state: BrainState,
    pub position: Position,
    pub latest_snapshot: Option<String>,

    pub event_tx: broadcast::Sender<BrainEvent>,
    pub command_tx: mpsc::Sender<BrainCommand>,
    command_rx: Option<mpsc::Receiver<BrainCommand>>,

    stream: Option<MemoryStream>,
    config: Config,

    seen_env_files: HashSet<String>,
    inbox_pending: Vec<NewFileInfo>,
    cycles_since_plan: u32,
    current_focus: String,
    focus_mode: bool,
    consecutive_research_cycles: u32,

    user_message: Option<String>,
    waiting_for_reply: bool,
    conversation_reply: Option<tokio::sync::oneshot::Sender<String>>,
}

impl Brain {
    pub fn new(identity: Identity, env_path: PathBuf, config: Config) -> Self {
        let (event_tx, _) = broadcast::channel(256);
        let (command_tx, command_rx) = mpsc::channel(32);

        Self {
            identity,
            env_path,
            events: Vec::new(),
            api_calls: Vec::new(),
            thought_count: 0,
            state: BrainState::Idle,
            position: Position { x: 5, y: 5 },
            latest_snapshot: None,
            event_tx,
            command_tx,
            command_rx: Some(command_rx),
            stream: None,
            config,
            seen_env_files: HashSet::new(),
            inbox_pending: Vec::new(),
            cycles_since_plan: 0,
            current_focus: String::new(),
            focus_mode: false,
            consecutive_research_cycles: 0,
            user_message: None,
            waiting_for_reply: false,
            conversation_reply: None,
        }
    }

    pub fn subscribe(&self) -> broadcast::Receiver<BrainEvent> {
        self.event_tx.subscribe()
    }

    pub fn command_sender(&self) -> mpsc::Sender<BrainCommand> {
        self.command_tx.clone()
    }

    pub fn is_waiting_for_reply(&self) -> bool {
        self.waiting_for_reply
    }

    fn broadcast(&self, event: BrainEvent) {
        let _ = self.event_tx.send(event);
    }

    fn stream(&self) -> &MemoryStream {
        self.stream.as_ref().expect("stream not initialized")
    }

    fn stream_mut(&mut self) -> &mut MemoryStream {
        self.stream.as_mut().expect("stream not initialized")
    }

    // ── Event helpers ──

    fn emit(&mut self, event_type: &str, data: serde_json::Value) {
        let entry = EventEntry {
            event_type: event_type.to_string(),
            timestamp: chrono::Utc::now().to_rfc3339(),
            thought_number: self.thought_count,
            data: data.clone(),
        };
        self.events.push(entry.clone());
        self.broadcast(BrainEvent::Entry(entry));

        let text = data
            .get("text")
            .or_else(|| data.get("command"))
            .or_else(|| data.get("content"))
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let truncated: String = text.chars().take(120).collect();
        info!("[{}] {}", event_type, truncated);
    }

    fn emit_api_call(
        &mut self,
        instructions: &str,
        input_list: &[serde_json::Value],
        response: &LlmResponse,
        is_reflection: bool,
        is_planning: bool,
    ) {
        let record = ApiCallRecord {
            timestamp: chrono::Utc::now().to_rfc3339(),
            instructions: instructions.to_string(),
            input: input_list.to_vec(),
            output: response.output.clone(),
            is_reflection,
            is_planning,
        };
        self.api_calls.push(record.clone());
        self.broadcast(BrainEvent::ApiCall(record));
    }

    // ── Activity classification (1:1 with Python) ──

    fn classify_activity(tool_name: &str, tool_args: &serde_json::Value) -> ActivityData {
        match tool_name {
            "move" => {
                let loc = tool_args
                    .get("location")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                ActivityData {
                    activity_type: "moving".to_string(),
                    detail: format!("Going to {}", loc),
                }
            }
            "respond" => ActivityData {
                activity_type: "conversing".to_string(),
                detail: "Talking to someone...".to_string(),
            },
            "fetch_url" | "web_search" | "web_fetch" => ActivityData {
                activity_type: "searching".to_string(),
                detail: format!("{}...", tool_name.replace('_', " ")),
            },
            "shell" => {
                let cmd = tool_args
                    .get("command")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .trim();
                if cmd.starts_with("python") {
                    let detail: String = cmd.chars().take(60).collect();
                    ActivityData {
                        activity_type: "python".to_string(),
                        detail: if cmd.len() > 60 {
                            format!("{}...", detail)
                        } else {
                            detail
                        },
                    }
                } else if cmd.contains('>') || cmd.starts_with("cat >") || cmd.starts_with("tee ") {
                    let fname = cmd
                        .split('>')
                        .last()
                        .and_then(|s| s.trim().split_whitespace().next())
                        .unwrap_or("file");
                    ActivityData {
                        activity_type: "writing".to_string(),
                        detail: format!("Writing {}", fname),
                    }
                } else if cmd.starts_with("cat ")
                    || cmd.starts_with("head ")
                    || cmd.starts_with("tail ")
                    || cmd.starts_with("ls")
                    || cmd.starts_with("find ")
                    || cmd.starts_with("grep ")
                {
                    let detail: String = cmd.chars().take(50).collect();
                    ActivityData {
                        activity_type: "reading".to_string(),
                        detail,
                    }
                } else {
                    let detail: String = cmd.chars().take(50).collect();
                    ActivityData {
                        activity_type: "shell".to_string(),
                        detail,
                    }
                }
            }
            _ => ActivityData {
                activity_type: "working".to_string(),
                detail: tool_name.to_string(),
            },
        }
    }

    // ── Input building (1:1 with Python) ──

    fn build_input(&self) -> (String, Vec<serde_json::Value>) {
        let instructions = main_system_prompt(&self.identity, &self.current_focus);
        let mut input_list: Vec<serde_json::Value> = Vec::new();

        // Recent events as context
        let recent: Vec<&EventEntry> = self
            .events
            .iter()
            .filter(|e| {
                e.event_type == "thought"
                    || e.event_type == "tool_call"
                    || e.event_type == "reflection"
            })
            .collect();
        let start = recent.len().saturating_sub(self.config.max_thoughts_in_context);
        for ev in &recent[start..] {
            match ev.event_type.as_str() {
                "thought" => {
                    let text = ev.data.get("text").and_then(|v| v.as_str()).unwrap_or("");
                    input_list.push(json!({"role": "assistant", "content": text}));
                }
                "tool_call" => {
                    let tool = ev.data.get("tool").and_then(|v| v.as_str()).unwrap_or("");
                    input_list.push(json!({"role": "assistant", "content": format!("[Used {} tool]", tool)}));
                }
                "reflection" => {
                    let text = ev.data.get("text").and_then(|v| v.as_str()).unwrap_or("");
                    let truncated: String = text.chars().take(200).collect();
                    input_list.push(json!({"role": "assistant", "content": format!("[Reflection: {}...]", truncated)}));
                }
                _ => {}
            }
        }

        let nudge = if self.thought_count == 0 && recent.is_empty() {
            self.build_wake_nudge()
        } else {
            self.build_continue_nudge()
        };

        // User message overrides nudge
        let final_nudge = if let Some(ref msg) = self.user_message {
            format!(
                "You hear a voice from outside your room say: \"{}\"\n\nYou can respond with the respond tool, or just keep doing what you're doing.",
                msg
            )
        } else {
            nudge
        };

        // Inbox pending overrides nudge
        if !self.inbox_pending.is_empty() {
            let names: Vec<&str> = self.inbox_pending.iter().map(|f| f.name.as_str()).collect();
            let mut parts = vec![format!(
                "YOUR OWNER left something for you! New file(s): {}\n\n\
                This is a gift from the outside world — DROP EVERYTHING and focus on it. \
                Your owner took the time to give this to you, so give it your full attention.\n\n\
                Here's what to do:\n\
                1. Read/examine it thoroughly — understand what it is and why they gave it to you\n\
                2. Think about what would be MOST USEFUL to do with it\n\
                3. Make a plan: what research, analysis, or projects could come from this?\n\
                4. Start executing — write summaries, do related web searches, build something inspired by it\n\
                5. Use the respond tool to tell your owner what you found and what you're doing with it\n\n\
                Spend your next several think cycles on this. Don't just glance at it and move on.",
                names.join(", ")
            )];
            for f in &self.inbox_pending {
                if f.image.is_some() {
                    parts.push(format!("\n📎 {} (image attached below)", f.name));
                } else if !f.content.is_empty() {
                    parts.push(format!("\n📎 {}:\n{}", f.name, f.content));
                }
            }
            input_list.push(json!({"role": "user", "content": parts.join("\n")}));
        } else {
            input_list.push(json!({"role": "user", "content": final_nudge}));
        }

        (instructions, input_list)
    }

    fn build_wake_nudge(&self) -> String {
        let mut parts = vec!["You're waking up. Here's your world:\n".to_string()];

        // Read projects.md
        if let Ok(projects) = std::fs::read_to_string(self.env_path.join("projects.md")) {
            let truncated: String = projects.chars().take(1500).collect();
            parts.push(format!("**Your projects (projects.md):**\n{}", truncated));
        } else {
            parts.push(
                "**No projects.md yet.** Create one to track what you're working on!".to_string(),
            );
        }

        // List files
        let files = self.list_env_files();
        if !files.is_empty() {
            let listing = files
                .iter()
                .take(30)
                .map(|f| format!("  {}", f))
                .collect::<Vec<_>>()
                .join("\n");
            parts.push(format!("**Files in your world:**\n{}", listing));
        }

        // Retrieve memories
        let memories = self.stream().retrieve_sync("what was I working on and thinking about", Some(5));
        if !memories.is_empty() {
            let mem_text = memories
                .iter()
                .map(|m| format!("- {}", m.content))
                .collect::<Vec<_>>()
                .join("\n");
            parts.push(format!("**Memories from before:**\n{}", mem_text));
        }

        parts.push(
            "\nCheck your projects. Pick up where you left off, or start something new.".to_string(),
        );
        parts.join("\n\n")
    }

    fn build_continue_nudge(&self) -> String {
        if self.focus_mode {
            return format!("Continue.\n{}", FOCUS_NUDGE);
        }

        let mut parts = Vec::new();

        // Research nudge
        if self.consecutive_research_cycles >= 5 {
            parts.push(
                "IMPORTANT: You've been researching for many cycles \
                without writing any files. STOP researching. Write up \
                what you've found NOW — save a report, summary, or \
                analysis to a file using a shell command."
                    .to_string(),
            );
        } else if self.consecutive_research_cycles >= 3 {
            parts.push(
                "You've gathered good research material. Time to \
                write up your findings — save a report or summary \
                to a file (e.g. research/topic_name.md)."
                    .to_string(),
            );
        }

        // Current focus
        if !self.current_focus.is_empty() {
            parts.push(format!("Current focus: {}", self.current_focus));
        }

        // Related memories from last thought
        let last_thought = self
            .events
            .iter()
            .rev()
            .find(|e| e.event_type == "thought")
            .and_then(|e| e.data.get("text").and_then(|v| v.as_str()));

        if let Some(thought) = last_thought {
            let memories = self.stream().retrieve_sync(thought, Some(3));
            let now = chrono::Utc::now();
            let older: Vec<&&Memory> = memories
                .iter()
                .filter(|m| {
                    chrono::DateTime::parse_from_rfc3339(&m.timestamp)
                        .map(|t| (now - t.with_timezone(&chrono::Utc)).num_seconds() > 30)
                        .unwrap_or(false)
                })
                .collect();
            if !older.is_empty() {
                let mem_text = older
                    .iter()
                    .map(|m| format!("- {}", m.content))
                    .collect::<Vec<_>>()
                    .join("\n");
                parts.push(format!("Related memories:\n{}", mem_text));
            }
        }

        if parts.is_empty() {
            "Continue.".to_string()
        } else {
            format!("Continue.\n{}", parts.join("\n"))
        }
    }

    // ── Think cycle (1:1 with Python _think_once) ──

    async fn think_once(&mut self) {
        self.state = BrainState::Thinking;
        self.broadcast(BrainEvent::Status(StatusData {
            state: BrainState::Thinking,
            thought_count: self.thought_count,
        }));

        let (instructions, mut input_list) = self.build_input();

        // Clear user message after building input
        self.user_message = None;

        let max_tokens = self.config.max_output_tokens;
        let response = match providers::chat(
            &self.config,
            &input_list,
            true,
            Some(&instructions),
            max_tokens,
        )
        .await
        {
            Ok(r) => r,
            Err(e) => {
                error!("LLM call failed: {}", e);
                self.emit("error", json!({"text": e.to_string()}));
                return;
            }
        };

        self.emit_api_call(&instructions, &input_list, &response, false, false);

        let pre_cycle_files = self.scan_env_files();
        let mut did_research = false;

        let max_tool_rounds = self.config.max_tool_rounds;
        let mut tool_round = 0;
        let mut current_response = response;

        while !current_response.tool_calls.is_empty() {
            tool_round += 1;
            if tool_round > max_tool_rounds {
                warn!("Hit max tool rounds ({}), stopping tool loop", max_tool_rounds);
                break;
            }

            if let Some(ref text) = current_response.text {
                self.emit("thought", json!({"text": text}));
            }

            // Append output to input_list
            input_list.extend(current_response.output.clone());

            for tc in &current_response.tool_calls {
                if matches!(tc.name.as_str(), "web_search" | "web_fetch" | "fetch_url") {
                    did_research = true;
                }

                self.emit(
                    "tool_call",
                    json!({"tool": &tc.name, "args": &tc.arguments}),
                );

                let activity = Self::classify_activity(&tc.name, &tc.arguments);
                self.broadcast(BrainEvent::Activity(activity));

                let pre_tool_files = self.scan_env_files();

                let result = match tc.name.as_str() {
                    "move" => {
                        let location = tc
                            .arguments
                            .get("location")
                            .and_then(|v| v.as_str())
                            .unwrap_or("center");
                        let result =
                            crate::tools::movement::handle_move(&mut self.position, location);
                        self.broadcast(BrainEvent::Position(self.position.clone()));
                        result
                    }
                    "respond" => {
                        let message = tc
                            .arguments
                            .get("message")
                            .and_then(|v| v.as_str())
                            .unwrap_or("");
                        self.handle_respond(message).await
                    }
                    _ => match tools::execute_tool(&tc.name, &tc.arguments, &self.env_path).await {
                        Ok(r) => r,
                        Err(e) => format!("Error: {}", e),
                    },
                };

                self.broadcast(BrainEvent::Activity(ActivityData {
                    activity_type: "idle".to_string(),
                    detail: String::new(),
                }));
                self.emit(
                    "tool_result",
                    json!({"tool": &tc.name, "output": &result}),
                );

                // Track files created by the anemone
                let post_tool_files = self.scan_env_files();
                for f in post_tool_files.difference(&pre_tool_files) {
                    self.seen_env_files.insert(f.clone());
                }

                input_list.push(json!({
                    "type": "function_call_output",
                    "call_id": &tc.call_id,
                    "name": &tc.name,
                    "output": &result,
                }));
            }

            // Follow-up LLM call
            current_response = match providers::chat(
                &self.config,
                &input_list,
                true,
                Some(&instructions),
                max_tokens,
            )
            .await
            {
                Ok(r) => r,
                Err(e) => {
                    // Retry on 500
                    if e.to_string().contains("500") {
                        warn!("LLM 500, retrying: {}", e);
                        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                        match providers::chat(
                            &self.config,
                            &input_list,
                            true,
                            Some(&instructions),
                            max_tokens,
                        )
                        .await
                        {
                            Ok(r) => r,
                            Err(e2) => {
                                error!("LLM follow-up failed after retry: {}", e2);
                                self.emit("error", json!({"text": e2.to_string()}));
                                break;
                            }
                        }
                    } else {
                        error!("LLM follow-up call failed: {}", e);
                        self.emit("error", json!({"text": e.to_string()}));
                        break;
                    }
                }
            };

            self.emit_api_call(&instructions, &input_list, &current_response, false, false);
        }

        // Track research-to-output ratio
        let post_cycle_files = self.scan_env_files();
        let created_files: HashSet<_> = post_cycle_files.difference(&pre_cycle_files).collect();
        if !created_files.is_empty() {
            self.consecutive_research_cycles = 0;
            info!("Files created this cycle: {:?}", created_files);
        } else if did_research {
            self.consecutive_research_cycles += 1;
            info!(
                "Research cycle with no file output ({} consecutive)",
                self.consecutive_research_cycles
            );
        }

        if let Some(ref text) = current_response.text {
            self.thought_count += 1;
            self.emit("thought", json!({"text": text}));

            // Store in memory stream
            if let Err(e) = self
                .stream_mut()
                .add(text, "thought", 0, Vec::new())
                .await
            {
                error!("Memory add failed: {}", e);
            }
        }
    }

    // ── Conversation ──

    async fn handle_respond(&mut self, message: &str) -> String {
        self.waiting_for_reply = true;
        self.broadcast(BrainEvent::Conversation(ConversationData {
            state: "waiting".to_string(),
            message: Some(message.to_string()),
            timeout: Some(15),
        }));

        // Create oneshot channel for reply
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.conversation_reply = Some(tx);

        let reply = match tokio::time::timeout(
            std::time::Duration::from_secs(crate::tools::respond::CONVERSATION_TIMEOUT_SECS),
            rx,
        )
        .await
        {
            Ok(Ok(text)) => format!(
                "They say: \"{}\"\n(Use respond again to reply, or go back to what you were doing.)",
                text
            ),
            _ => "(They didn't say anything else. You can get back to what you were doing.)".to_string(),
        };

        self.waiting_for_reply = false;
        self.conversation_reply = None;
        self.broadcast(BrainEvent::Conversation(ConversationData {
            state: "ended".to_string(),
            message: None,
            timeout: None,
        }));

        reply
    }

    // ── Reflection (1:1 with Python) ──

    async fn reflect(&mut self) {
        self.state = BrainState::Reflecting;
        self.broadcast(BrainEvent::Status(StatusData {
            state: BrainState::Reflecting,
            thought_count: self.thought_count,
        }));
        self.emit("reflection_start", json!({}));

        let recent_memories: Vec<Memory> = self
            .stream()
            .get_recent(15, None)
            .into_iter()
            .cloned()
            .collect();

        if recent_memories.is_empty() {
            self.stream_mut().reset_importance_sum();
            return;
        }

        let memories_text = recent_memories
            .iter()
            .map(|m| format!("[{}] (importance {}): {}", m.kind, m.importance, m.content))
            .collect::<Vec<_>>()
            .join("\n\n");

        let reflect_input = vec![json!({
            "role": "user",
            "content": format!("Your recent memories:\n\n{}", memories_text)
        })];

        match providers::chat(
            &self.config,
            &reflect_input,
            false,
            Some(REFLECTION_PROMPT),
            300,
        )
        .await
        {
            Ok(response) => {
                self.emit_api_call(REFLECTION_PROMPT, &reflect_input, &response, true, false);

                let reflection_text = response.text.unwrap_or_default();
                let source_ids: Vec<String> =
                    recent_memories.iter().map(|m| m.id.clone()).collect();

                for line in reflection_text.lines() {
                    let insight = line.trim();
                    if !insight.is_empty() {
                        if let Err(e) = self
                            .stream_mut()
                            .add(insight, "reflection", 1, source_ids.clone())
                            .await
                        {
                            error!("Failed to store reflection: {}", e);
                        }
                    }
                }

                self.emit("reflection", json!({"text": &reflection_text}));
            }
            Err(e) => {
                error!("Reflection failed: {}", e);
                self.emit("error", json!({"text": format!("Reflection failed: {}", e)}));
            }
        }

        self.stream_mut().reset_importance_sum();
    }

    // ── Planning (1:1 with Python) ──

    async fn plan(&mut self) {
        self.state = BrainState::Planning;
        self.broadcast(BrainEvent::Status(StatusData {
            state: BrainState::Planning,
            thought_count: self.thought_count,
        }));

        let projects = std::fs::read_to_string(self.env_path.join("projects.md"))
            .unwrap_or_else(|_| "(no projects.md yet)".to_string());
        let projects_truncated: String = projects.chars().take(2000).collect();

        let files = self.list_env_files();
        let files_str = if files.is_empty() {
            "(empty)".to_string()
        } else {
            files.iter().take(30).cloned().collect::<Vec<_>>().join("\n")
        };

        let recent_memories: Vec<String> = self
            .stream()
            .get_recent(10, None)
            .iter()
            .map(|m| format!("- {}", m.content))
            .collect();
        let memories_text = if recent_memories.is_empty() {
            "(none yet)".to_string()
        } else {
            recent_memories.join("\n")
        };

        let plan_input = vec![json!({
            "role": "user",
            "content": format!(
                "Time to plan. Here's your current state:\n\n\
                ## Current projects.md:\n{}\n\n\
                ## Files in your world:\n{}\n\n\
                ## Recent thoughts:\n{}",
                projects_truncated, files_str, memories_text
            )
        })];

        match providers::chat(&self.config, &plan_input, false, Some(PLANNING_PROMPT), 1000).await {
            Ok(response) => {
                self.emit_api_call(PLANNING_PROMPT, &plan_input, &response, false, true);

                let plan_text = response.text.unwrap_or_default();
                if plan_text.is_empty() {
                    return;
                }

                // Split plan from log entry (separated by "LOG:")
                let (plan_body, log_entry) = if let Some(idx) = plan_text.find("LOG:") {
                    (plan_text[..idx].trim().to_string(), plan_text[idx + 4..].trim().to_string())
                } else {
                    (plan_text.clone(), String::new())
                };

                // Write projects.md
                if let Err(e) = std::fs::write(self.env_path.join("projects.md"), &plan_body) {
                    error!("Failed to write projects.md: {}", e);
                }

                // Append daily log entry
                if !log_entry.is_empty() {
                    let log_dir = self.env_path.join("logs");
                    let _ = std::fs::create_dir_all(&log_dir);
                    let today = chrono::Local::now().format("%Y-%m-%d").to_string();
                    let log_path = log_dir.join(format!("{}.md", today));
                    let now_str = chrono::Local::now().format("%I:%M %p").to_string();
                    if let Err(e) = std::fs::OpenOptions::new()
                        .create(true)
                        .append(true)
                        .open(&log_path)
                        .and_then(|mut f| {
                            use std::io::Write;
                            writeln!(f, "\n## {}\n{}", now_str, log_entry)
                        })
                    {
                        error!("Failed to write daily log: {}", e);
                    }
                }

                self.current_focus = self.load_current_focus();
                self.cycles_since_plan = 0;
                self.seen_env_files = self.scan_env_files();
                self.emit("planning", json!({"text": &plan_text}));
            }
            Err(e) => {
                error!("Planning failed: {}", e);
                self.emit("error", json!({"text": format!("Planning failed: {}", e)}));
            }
        }
    }

    // ── Main loop ──

    pub async fn run(&mut self) {
        info!("{} is waking up...", self.identity.name);

        crate::tools::shell::ensure_venv(&self.env_path);
        self.stream = Some(MemoryStream::new(&self.env_path, self.config.clone()));

        // Initial file scan — mark subdirectory files as "seen" but leave root-level
        // user files unseen so they trigger inbox alerts
        let all_files = self.scan_env_files();
        self.seen_env_files = all_files
            .into_iter()
            .filter(|f| f.contains(std::path::MAIN_SEPARATOR) || INTERNAL_ROOT_FILES.contains(&f.as_str()))
            .collect();
        self.current_focus = self.load_current_focus();

        info!("{} is ready.", self.identity.name);

        let mut command_rx = self.command_rx.take().expect("command_rx already taken");
        let mut running = true;

        while running {
            // Process commands
            while let Ok(cmd) = command_rx.try_recv() {
                match cmd {
                    BrainCommand::UserMessage(text) => {
                        self.user_message = Some(text);
                    }
                    BrainCommand::ConversationReply(text) => {
                        if let Some(tx) = self.conversation_reply.take() {
                            let _ = tx.send(text);
                        }
                    }
                    BrainCommand::SetFocusMode(enabled) => {
                        self.focus_mode = enabled;
                        self.broadcast(BrainEvent::FocusMode(FocusModeData { enabled }));
                    }
                    BrainCommand::Snapshot(data) => {
                        self.latest_snapshot = Some(data);
                    }
                    BrainCommand::Stop => {
                        running = false;
                        break;
                    }
                }
            }

            if !running {
                break;
            }

            // Check for new files
            let new_files = self.check_new_files();
            if !new_files.is_empty() {
                self.inbox_pending = new_files;
                self.broadcast(BrainEvent::Alert);
            }

            // Think
            self.think_once().await;

            // Clear inbox after thinking
            self.inbox_pending.clear();

            // Reflect if needed
            if self.stream().should_reflect() {
                self.reflect().await;
            }

            // Plan periodically
            self.cycles_since_plan += 1;
            if self.cycles_since_plan >= PLAN_INTERVAL {
                self.plan().await;
            }

            // Idle
            self.state = BrainState::Idle;
            self.broadcast(BrainEvent::Status(StatusData {
                state: BrainState::Idle,
                thought_count: self.thought_count,
            }));

            // Idle wander
            crate::tools::movement::idle_wander(&mut self.position);
            self.broadcast(BrainEvent::Position(self.position.clone()));

            tokio::time::sleep(std::time::Duration::from_secs(
                self.config.thinking_pace_seconds,
            ))
            .await;
        }

        info!("{} is shutting down.", self.identity.name);
        self.state = BrainState::Idle;
    }

    // ── File helpers ──

    fn scan_env_files(&self) -> HashSet<String> {
        let mut files = HashSet::new();
        if let Ok(entries) = walkdir(&self.env_path) {
            for entry in entries {
                let rel = entry
                    .strip_prefix(&self.env_path)
                    .unwrap_or(&entry);
                let rel_str = rel.to_string_lossy().to_string();
                let fname = rel
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("");
                if fname.starts_with('.') || IGNORE_FILES.contains(&fname) {
                    continue;
                }
                files.insert(rel_str);
            }
        }
        files
    }

    fn check_new_files(&mut self) -> Vec<NewFileInfo> {
        let current = self.scan_env_files();
        let new_paths: HashSet<_> = current.difference(&self.seen_env_files).cloned().collect();
        self.seen_env_files = current;

        let mut results = Vec::new();
        for rel_path in new_paths {
            let fpath = self.env_path.join(&rel_path);
            if !fpath.is_file() {
                continue;
            }
            let ext = fpath
                .extension()
                .and_then(|e| e.to_str())
                .map(|e| format!(".{}", e.to_lowercase()))
                .unwrap_or_default();

            let mut entry = NewFileInfo {
                name: rel_path.clone(),
                content: String::new(),
                image: None,
            };

            if crate::tools::shell::TEXT_EXTS.contains(&ext.as_str()) {
                entry.content = std::fs::read_to_string(&fpath)
                    .map(|s| {
                        let truncated: String = s.chars().take(2000).collect();
                        truncated
                    })
                    .unwrap_or_else(|_| "(could not read file)".to_string());
            } else if crate::tools::shell::IMAGE_EXTS.contains(&ext.as_str()) {
                if let Ok(data) = std::fs::read(&fpath) {
                    let mime = match ext.as_str() {
                        ".png" => "image/png",
                        ".jpg" | ".jpeg" => "image/jpeg",
                        ".gif" => "image/gif",
                        ".webp" => "image/webp",
                        _ => "application/octet-stream",
                    };
                    use base64::Engine;
                    let b64 = base64::engine::general_purpose::STANDARD.encode(&data);
                    entry.image = Some(format!("data:{};base64,{}", mime, b64));
                }
            } else {
                entry.content = format!("(binary file: {})", rel_path);
            }

            results.push(entry);
        }
        results
    }

    fn load_current_focus(&self) -> String {
        let projects_path = self.env_path.join("projects.md");
        if let Ok(content) = std::fs::read_to_string(&projects_path) {
            let mut in_focus = false;
            let mut focus_lines = Vec::new();
            for line in content.lines() {
                if line.trim().to_lowercase().starts_with("# current focus") {
                    in_focus = true;
                    continue;
                }
                if in_focus {
                    if line.starts_with("# ") {
                        break;
                    }
                    let trimmed = line.trim();
                    if !trimmed.is_empty() {
                        focus_lines.push(trimmed.to_string());
                    }
                }
            }
            let focus = focus_lines.join(" ");
            let truncated: String = focus.chars().take(300).collect();
            truncated
        } else {
            String::new()
        }
    }

    fn list_env_files(&self) -> Vec<String> {
        let mut files: Vec<String> = self.scan_env_files().into_iter().collect();
        files.sort();
        files
    }
}

/// Walk a directory recursively, returning all file paths.
fn walkdir(root: &Path) -> std::io::Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    if !root.is_dir() {
        return Ok(files);
    }
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir)? {
            let entry = entry?;
            let path = entry.path();
            let name = entry.file_name();
            if name.to_string_lossy().starts_with('.') {
                continue;
            }
            if path.is_dir() {
                stack.push(path);
            } else {
                files.push(path);
            }
        }
    }
    Ok(files)
}
