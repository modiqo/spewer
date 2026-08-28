#![doc = "Spewer supervises bounded work delegated to agent harnesses."]
#![forbid(unsafe_code)]

/// The public wire protocol shared with parent harnesses.
pub mod protocol;
/// Deterministic task projection and state transitions.
pub mod reducer;
/// End-to-end bounded task execution.
pub mod runner;
/// Durable event log, projection, and source deduplication.
pub mod store;
/// Isolated Git worktree preparation and artifact capture.
pub mod workspace;

/// Command-line parsing and dispatch.
pub mod cli;
/// Codex App Server process and protocol integration.
pub mod codex;
/// Typed failures returned by Spewer operations.
pub mod error;

mod receipt;
mod util;
