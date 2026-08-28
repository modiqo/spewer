#![doc = "Spewer supervises bounded work delegated to agent harnesses."]
#![forbid(unsafe_code)]

/// The public wire protocol shared with parent harnesses.
pub mod protocol;

/// Command-line parsing and dispatch.
pub mod cli;
/// Codex App Server process and protocol integration.
pub mod codex;
/// Typed failures returned by Spewer operations.
pub mod error;
