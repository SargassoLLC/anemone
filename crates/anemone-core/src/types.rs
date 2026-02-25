//! Core types — BrainState, Memory, Identity, ToolCall, LlmResponse, etc.

use serde::{Deserialize, Serialize};

// ── Brain state ──

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BrainState {
    Idle,
    Thinking,
    Reflecting,
    Planning,
}

impl std::fmt::Display for BrainState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BrainState::Idle => write!(f, "idle"),
            BrainState::Thinking => write!(f, "thinking"),
            BrainState::Reflecting => write!(f, "reflecting"),
            BrainState::Planning => write!(f, "planning"),
        }
    }
}

// ── Identity ──

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Traits {
    pub domains: Vec<String>,
    pub thinking_styles: Vec<String>,
    pub temperament: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Identity {
    pub name: String,
    pub genome: String,
    pub traits: Traits,
    pub born: String,
}

// ── Memory ──

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Memory {
    pub id: String,
    pub timestamp: String,
    pub kind: String,
    pub content: String,
    pub importance: i32,
    pub depth: i32,
    pub references: Vec<String>,
    #[serde(default)]
    pub embedding: Vec<f64>,
}

// ── Tool definitions ──

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub name: String,
    pub arguments: serde_json::Value,
    pub call_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmResponse {
    pub text: Option<String>,
    pub tool_calls: Vec<ToolCall>,
    /// Raw output items for appending back to input on follow-up calls
    pub output: Vec<serde_json::Value>,
}

// ── Events (broadcast from Brain to frontends) ──

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Position {
    pub x: i32,
    pub y: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatusData {
    pub state: BrainState,
    pub thought_count: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActivityData {
    #[serde(rename = "type")]
    pub activity_type: String,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversationData {
    pub state: String, // "waiting" | "ended"
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timeout: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FocusModeData {
    pub enabled: bool,
}

// ── Event entry (stored in events list) ──

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventEntry {
    #[serde(rename = "type")]
    pub event_type: String,
    pub timestamp: String,
    pub thought_number: u32,
    #[serde(flatten)]
    pub data: serde_json::Value,
}

// ── API call record ──

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiCallRecord {
    pub timestamp: String,
    pub instructions: String,
    pub input: Vec<serde_json::Value>,
    pub output: Vec<serde_json::Value>,
    #[serde(rename = "is_dream")]
    pub is_reflection: bool,
    pub is_planning: bool,
}

// ── New file info (for inbox alerts) ──

#[derive(Debug, Clone)]
pub struct NewFileInfo {
    pub name: String,
    pub content: String,
    pub image: Option<String>, // data URL
}

// ── Anemone info (for listing) ──

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnemoneInfo {
    pub id: String,
    pub name: String,
    pub state: BrainState,
    pub thought_count: u32,
}

// ── Room locations ──

pub const ROOM_LOCATIONS: &[(&str, i32, i32)] = &[
    ("desk", 10, 1),
    ("bookshelf", 1, 2),
    ("window", 4, 0),
    ("plant", 0, 8),
    ("bed", 3, 10),
    ("rug", 5, 5),
    ("center", 5, 5),
];

pub fn room_location(name: &str) -> Option<Position> {
    ROOM_LOCATIONS
        .iter()
        .find(|(n, _, _)| *n == name)
        .map(|(_, x, y)| Position { x: *x, y: *y })
}

/// Valid location names for the move tool enum
pub const LOCATION_NAMES: &[&str] = &[
    "desk",
    "bookshelf",
    "window",
    "plant",
    "bed",
    "rug",
    "center",
];
