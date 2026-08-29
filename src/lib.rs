#![doc = "Spewer supervises bounded work delegated to agent harnesses."]
#![forbid(unsafe_code)]

/// Hard time, token, tool, retry, and cost boundaries.
pub mod budget;
/// Durable callbacks and parent acknowledgement types.
pub mod delivery;
/// Provider-neutral harness capability and event boundary.
pub mod engine;
/// Deterministic second engine used by the conformance suite.
pub mod fake;
/// Local open-weights inference through Ollama.
pub mod ollama;
/// Play-compatible handoff and exactly-once parent application.
pub mod parent;
/// The public wire protocol shared with parent harnesses.
pub mod protocol;
/// Checkpoint validation and interrupted-run reconciliation.
pub mod recovery;
/// Deterministic task projection and state transitions.
pub mod reducer;
/// End-to-end bounded task execution.
pub mod runner;
/// Redaction and idempotent external-effect policy.
pub mod security;
/// Durable event log, projection, and source deduplication.
pub mod store;
/// Turn-aware scheduling across bounded App Server workers.
pub mod supervisor;
/// Cost derivation and Pareto comparison exports.
pub mod telemetry;
/// Isolated Git worktree preparation and artifact capture.
pub mod workspace;

/// Durable generic and skill-specialized worker descriptions.
pub mod capsule;
/// Command-line parsing and dispatch.
pub mod cli;
/// Codex App Server process and protocol integration.
pub mod codex;
/// Owner-private defaults used to infer one-off questions.
pub mod config;
/// Private local control socket used by the CLI and parent harnesses.
pub mod control;
/// Typed failures returned by Spewer operations.
pub mod error;
/// Reusable discovery, delegation, checking, and cancellation client.
pub mod harness;

mod journal;
mod receipt;
mod resume;
mod util;
