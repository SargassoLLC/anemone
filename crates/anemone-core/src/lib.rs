//! anemone-core — Pure domain logic, no UI.
//!
//! This crate contains the complete brain, memory, identity, and tool logic
//! for the Anemone autonomous AI agent. It is completely UI-agnostic —
//! frontends (TUI, Web) subscribe to events via tokio::broadcast.

pub mod config;
pub mod events;
pub mod identity;
pub mod prompts;
pub mod types;

// These modules will be implemented in later phases:
pub mod brain;
pub mod memory;
pub mod providers;
pub mod tools;
