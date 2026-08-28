//! Engine-neutral request, event, checkpoint, and receipt types.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fmt;
use std::path::{Component, Path};

/// The current wire protocol version.
pub const PROTOCOL_VERSION: &str = "0.1";

/// A request violated a public protocol invariant.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProtocolError {
    message: String,
}

impl ProtocolError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl fmt::Display for ProtocolError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for ProtocolError {}

/// A bounded task submitted by a parent harness.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct TaskRequest {
    /// Wire protocol version.
    pub protocol_version: String,
    /// Optional stable identifier supplied by the parent.
    #[serde(default)]
    pub task_id: Option<String>,
    /// Stable parent key used to deduplicate submissions.
    pub idempotency_key: String,
    /// The bounded outcome delegated to the worker.
    pub objective: String,
    /// Observable checks that define success.
    #[serde(default)]
    pub acceptance: Vec<String>,
    /// Repository and revision to operate on.
    pub workspace: WorkspaceRequest,
    /// Projected context visible to the worker.
    #[serde(default)]
    pub context: TaskContext,
    /// Authority granted to the worker.
    pub permissions: Permissions,
    /// Hard execution limits.
    pub budgets: Budgets,
    /// Requested worker engine and model.
    pub engine: EngineRequest,
    /// Parent delivery preference.
    pub callback: CallbackRequest,
    /// Parent-owned continuation state that Spewer never interprets.
    #[serde(default)]
    pub private_continuation: Option<Value>,
}

impl TaskRequest {
    /// Validates semantics that Serde cannot express.
    pub fn validate(&self) -> Result<(), ProtocolError> {
        if self.protocol_version != PROTOCOL_VERSION {
            return Err(ProtocolError::new("unsupported protocol_version"));
        }
        nonempty("idempotency_key", &self.idempotency_key)?;
        nonempty("objective", &self.objective)?;
        if self.idempotency_key.len() > 512 {
            return Err(ProtocolError::new("idempotency_key exceeds 512 bytes"));
        }
        if self.objective.len() > 65_536 {
            return Err(ProtocolError::new("objective exceeds 65536 bytes"));
        }
        if self.acceptance.len() > 256 {
            return Err(ProtocolError::new("acceptance exceeds 256 entries"));
        }
        if let Some(task_id) = &self.task_id {
            nonempty("task_id", task_id)?;
        }
        let workspace = Path::new(&self.workspace.path);
        if !workspace.is_absolute() {
            return Err(ProtocolError::new("workspace.path must be absolute"));
        }
        validate_relative_paths("context.files", &self.context.files)?;
        validate_relative_paths(
            "permissions.writable_paths",
            &self.permissions.writable_paths,
        )?;
        match self.permissions.filesystem.as_str() {
            "read-only" | "workspace-write" => {}
            _ => return Err(ProtocolError::new("unsupported filesystem permission")),
        }
        match self.permissions.network.as_str() {
            "deny" | "allow" => {}
            _ => return Err(ProtocolError::new("unsupported network permission")),
        }
        match self.permissions.commands.as_str() {
            "engine-policy" => {}
            "allowlist" if !self.permissions.command_allowlist.is_empty() => {}
            "allowlist" => return Err(ProtocolError::new("command allowlist is empty")),
            _ => return Err(ProtocolError::new("unsupported command permission")),
        }
        if self.budgets.wall_seconds == 0
            || self.budgets.tokens == 0
            || self.budgets.tool_calls == 0
        {
            return Err(ProtocolError::new(
                "wall, token, and tool budgets must be positive",
            ));
        }
        if !self.budgets.cost_usd.is_finite() || self.budgets.cost_usd < 0.0 {
            return Err(ProtocolError::new(
                "cost budget must be finite and nonnegative",
            ));
        }
        match self.engine.kind.as_str() {
            "codex-app-server" | "fake" => {}
            _ => return Err(ProtocolError::new("unsupported engine kind")),
        }
        nonempty("engine.model", &self.engine.model)?;
        match self.callback.mode.as_str() {
            "stream" | "wait" | "poll" => Ok(()),
            _ => Err(ProtocolError::new("unsupported callback mode")),
        }
    }
}

fn nonempty(field: &str, value: &str) -> Result<(), ProtocolError> {
    if value.trim().is_empty() {
        return Err(ProtocolError::new(format!("{field} must not be empty")));
    }
    Ok(())
}

fn validate_relative_paths(field: &str, paths: &[String]) -> Result<(), ProtocolError> {
    for value in paths {
        let path = Path::new(value);
        if path.as_os_str().is_empty() || path.is_absolute() {
            return Err(ProtocolError::new(format!(
                "{field} must contain relative paths"
            )));
        }
        for component in path.components() {
            if matches!(
                component,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            ) {
                return Err(ProtocolError::new(format!(
                    "{field} contains an escaping path"
                )));
            }
        }
    }
    Ok(())
}

/// Repository coordinates for a task.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct WorkspaceRequest {
    /// Absolute repository path.
    pub path: String,
    /// Git revision used as the isolated worktree base.
    #[serde(default)]
    pub base_revision: Option<String>,
}

/// Context projected from the parent harness.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct TaskContext {
    /// Files useful to the task.
    #[serde(default)]
    pub files: Vec<String>,
    /// Parent-provided constraints or background.
    #[serde(default)]
    pub notes: Vec<String>,
}

/// Worker authority requested by the parent.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct Permissions {
    /// Filesystem policy: `read-only` or `workspace-write`.
    pub filesystem: String,
    /// Network policy: `deny` or `allow`.
    pub network: String,
    /// Command policy: `engine-policy` or `allowlist`.
    pub commands: String,
    /// Commands allowed when `commands` is `allowlist`.
    #[serde(default)]
    pub command_allowlist: Vec<String>,
    /// Environment variable names explicitly inherited by the engine.
    #[serde(default)]
    pub environment_allowlist: Vec<String>,
    /// Relative paths the worker may modify. Empty means the worktree.
    #[serde(default)]
    pub writable_paths: Vec<String>,
}

/// Hard limits for one task.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct Budgets {
    /// Maximum wall-clock seconds.
    pub wall_seconds: u64,
    /// Maximum total input and output tokens.
    pub tokens: u64,
    /// Maximum observed tool calls.
    pub tool_calls: u64,
    /// Maximum additional attempts after the first.
    pub retries: u32,
    /// Maximum derived provider cost in US dollars.
    pub cost_usd: f64,
}

/// Requested harness and model.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct EngineRequest {
    /// Engine discriminator.
    pub kind: String,
    /// Model selected after capability discovery.
    pub model: String,
    /// Optional provider reasoning effort.
    #[serde(default)]
    pub effort: Option<String>,
}

/// Parent result delivery preference.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct CallbackRequest {
    /// `stream`, `wait`, or `poll`.
    pub mode: String,
    /// Stable parent consumer identity used for acknowledgements.
    #[serde(default)]
    pub consumer_id: Option<String>,
}

/// A durable handle returned after task acceptance.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct TaskHandle {
    /// Wire protocol version.
    pub protocol_version: String,
    /// Durable task identifier.
    pub task_id: String,
    /// Current normalized task status.
    pub status: TaskStatus,
    /// Highest committed event sequence.
    pub event_cursor: u64,
    /// RFC 3339 creation timestamp.
    pub created_at: String,
}

/// Monotonic task states.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskStatus {
    /// Accepted but not started.
    Queued,
    /// Engine startup is in progress.
    Starting,
    /// The worker is acting.
    Running,
    /// The engine requires parent input.
    InputRequired,
    /// The engine is alive but silent beyond policy.
    Stalled,
    /// The task completed with evidence or a waiver.
    Completed,
    /// The task failed.
    Failed,
    /// The task was cancelled.
    Cancelled,
    /// The task requires frontier or human judgment.
    Escalated,
}

impl TaskStatus {
    /// Returns whether no later event may resume this attempt.
    pub const fn is_terminal(self) -> bool {
        matches!(
            self,
            Self::Completed | Self::Failed | Self::Cancelled | Self::Escalated
        )
    }
}

/// Normalized durable event shared with parent harnesses.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct Event {
    /// Wire protocol version.
    pub protocol_version: String,
    /// Durable task identifier.
    pub task_id: String,
    /// Attempt number, starting at one.
    pub attempt: u32,
    /// Gap-free per-task sequence.
    pub seq: u64,
    /// Stable normalized event type.
    #[serde(rename = "type")]
    pub kind: String,
    /// RFC 3339 observation timestamp.
    pub observed_at: String,
    /// Event-specific normalized data.
    pub data: Value,
    /// Optional engine provenance.
    #[serde(default)]
    pub source: Option<EventSource>,
}

/// Provenance retained for a normalized engine event.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct EventSource {
    /// Engine discriminator.
    pub engine: String,
    /// Native method or event name.
    pub method: String,
    /// Native thread identifier, when known.
    #[serde(default)]
    pub thread_id: Option<String>,
    /// Native turn identifier, when known.
    #[serde(default)]
    pub turn_id: Option<String>,
    /// Native item identifier, when known.
    #[serde(default)]
    pub item_id: Option<String>,
    /// SHA-256 identity of the redacted native payload.
    pub payload_hash: String,
}

/// Durable recovery boundary combining Spewer, engine, and workspace state.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct Checkpoint {
    /// Wire protocol version.
    pub protocol_version: String,
    /// Stable checkpoint identifier.
    pub checkpoint_id: String,
    /// Durable task identifier.
    pub task_id: String,
    /// Attempt number.
    pub attempt: u32,
    /// Event cursor covered by the checkpoint.
    pub event_seq: u64,
    /// Projection schema version.
    pub projection_version: u32,
    /// Namespaced engine recovery data.
    pub engine: Value,
    /// Workspace revision, diff, and artifact data.
    pub workspace: Value,
    /// Whether automated resumption is permitted.
    pub resumable: bool,
    /// Boundary that caused checkpoint creation.
    pub reason: String,
    /// RFC 3339 creation timestamp.
    pub created_at: String,
}

/// Terminal status carried by a receipt.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ReceiptStatus {
    /// Acceptance criteria passed or were explicitly waived.
    Completed,
    /// The worker or verification failed.
    Failed,
    /// The parent or policy cancelled the task.
    Cancelled,
    /// Further judgment requires a stronger actor.
    Escalated,
}

/// One immutable artifact produced by the task.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct Artifact {
    /// Artifact media or semantic kind.
    pub kind: String,
    /// Content-addressed URI.
    pub uri: String,
    /// Lowercase SHA-256 digest.
    pub sha256: String,
}

/// One acceptance or verification result.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct Verification {
    /// Command or check name.
    pub command: String,
    /// Process exit code, when the verifier is a command.
    #[serde(default)]
    pub exit_code: Option<i32>,
    /// Hash of retained verifier output.
    #[serde(default)]
    pub output_sha256: Option<String>,
    /// Whether the check passed.
    pub passed: bool,
}

/// Provider facts and derived counters for an attempt.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct Usage {
    /// Provider-reported input tokens.
    pub input_tokens: Option<u64>,
    /// Provider-reported cached input tokens.
    pub cached_input_tokens: Option<u64>,
    /// Provider-reported output tokens.
    pub output_tokens: Option<u64>,
    /// Provider-reported reasoning tokens.
    pub reasoning_tokens: Option<u64>,
    /// Attempt wall time.
    pub wall_ms: u64,
    /// Observed tool calls.
    pub tool_calls: u64,
    /// Derived provider charge, when a matching price exists.
    pub actual_cost_usd: Option<f64>,
    /// Hash of the price configuration used for derived cost.
    pub price_config_hash: Option<String>,
}

/// Engine identity retained in a terminal receipt.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct ReceiptEngine {
    /// Engine discriminator.
    pub kind: String,
    /// Model requested after discovery.
    pub requested_model: String,
    /// Models actually observed, including reroutes.
    pub observed_models: Vec<String>,
    /// Upstream engine version.
    pub version: Option<String>,
}

/// Immutable terminal result delivered through the outbox.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct Receipt {
    /// Wire protocol version.
    pub protocol_version: String,
    /// Stable receipt identifier.
    pub receipt_id: String,
    /// Durable task identifier.
    pub task_id: String,
    /// Attempt number.
    pub attempt: u32,
    /// Terminal result category.
    pub status: ReceiptStatus,
    /// Worker result summary.
    pub summary: String,
    /// Immutable output artifacts.
    pub artifacts: Vec<Artifact>,
    /// Structured acceptance evidence.
    pub verification: Vec<Verification>,
    /// Explicit reason evidence was waived.
    pub verification_waiver: Option<String>,
    /// Tokens, cost, time, and tools used.
    pub usage: Usage,
    /// Requested and observed engine identity.
    pub engine: ReceiptEngine,
    /// Last committed event covered by the receipt.
    pub final_event_seq: u64,
    /// RFC 3339 completion timestamp.
    pub completed_at: String,
}
