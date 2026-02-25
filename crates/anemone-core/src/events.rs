//! BrainEvent enum — broadcast from Brain to TUI/Web frontends via tokio::broadcast.

use serde::{Deserialize, Serialize};

use crate::types::{
    ActivityData, ApiCallRecord, ConversationData, EventEntry, FocusModeData, Position, StatusData,
};

/// Events broadcast from a Brain task to all subscribers (TUI, WebSocket clients).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "event", content = "data")]
pub enum BrainEvent {
    /// A new event entry (thought, tool_call, tool_result, reflection, error, etc.)
    #[serde(rename = "entry")]
    Entry(EventEntry),

    /// Raw API call record (for the "raw" view / debugging)
    #[serde(rename = "api_call")]
    ApiCall(ApiCallRecord),

    /// Character position changed
    #[serde(rename = "position")]
    Position(Position),

    /// Brain state changed (idle/thinking/reflecting/planning)
    #[serde(rename = "status")]
    Status(StatusData),

    /// New file alert (owner dropped something in)
    #[serde(rename = "alert")]
    Alert,

    /// Current activity for frontend visualization
    #[serde(rename = "activity")]
    Activity(ActivityData),

    /// Focus mode toggled
    #[serde(rename = "focus_mode")]
    FocusMode(FocusModeData),

    /// Conversation state (waiting for reply / ended)
    #[serde(rename = "conversation")]
    Conversation(ConversationData),
}

impl BrainEvent {
    /// Serialize to the JSON format the frontend expects:
    /// `{"event": "...", "data": {...}}`
    pub fn to_ws_json(&self) -> serde_json::Value {
        serde_json::to_value(self).unwrap_or_default()
    }
}
