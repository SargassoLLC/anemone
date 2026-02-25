//! Conversation tool — respond to user with timeout.
//! The actual respond handling lives in Brain (Phase 3),
//! since it requires async waiting and broadcast channels.
//! This module provides the timeout constant and helper types.

/// Default conversation reply timeout in seconds.
pub const CONVERSATION_TIMEOUT_SECS: u64 = 15;
