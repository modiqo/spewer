//! Synchronous, deterministic task projection reducer.

use crate::error::{Error, ErrorKind, Result};
use crate::protocol::{Event, TaskRequest, TaskStatus, Usage};
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Projection schema version stored with tasks and checkpoints.
pub const PROJECTION_VERSION: u32 = 1;

/// Observable stage within a task status.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Phase {
    /// Waiting for engine startup.
    Starting,
    /// Worker tools or model are active.
    Acting,
    /// Acceptance checks are running.
    Verifying,
    /// Waiting for the parent or engine.
    Waiting,
    /// A terminal receipt is being delivered.
    Delivering,
}

/// One explicit engine plan entry.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct PlanStep {
    /// Engine-provided step text.
    pub step: String,
    /// `pending`, `in_progress`, or `completed`.
    pub status: String,
}

/// Engine identity and recoverable handles in the projection.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct EngineProjection {
    /// Engine discriminator.
    pub kind: String,
    /// Requested model.
    pub requested_model: String,
    /// Models observed during the attempt.
    pub observed_models: Vec<String>,
    /// Native thread identifier.
    pub thread_id: Option<String>,
    /// Native session-tree identifier.
    pub session_id: Option<String>,
    /// Native active turn identifier.
    pub turn_id: Option<String>,
}

/// Workspace evidence attached to current task state.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct WorkspaceProjection {
    /// Isolated worktree path.
    pub path: String,
    /// Immutable starting revision.
    pub base_revision: String,
    /// Hash of the current binary diff.
    pub diff_hash: Option<String>,
    /// Number of changed files.
    pub changed_files: u64,
}

/// Current state derived only from the event log.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct Projection {
    /// Projection schema version.
    pub version: u32,
    /// Durable task identifier.
    pub task_id: String,
    /// Current attempt number.
    pub attempt: u32,
    /// Current monotonic task state.
    pub status: TaskStatus,
    /// Observable task stage.
    pub phase: Phase,
    /// Highest applied event sequence.
    pub event_seq: u64,
    /// Explicit engine plan, when present.
    pub plan: Vec<PlanStep>,
    /// Current native item summary.
    pub active_item: Option<Value>,
    /// Latest worker result summary.
    pub summary: String,
    /// RFC 3339 task creation time.
    pub created_at: String,
    /// RFC 3339 latest activity time.
    pub last_activity_at: String,
    /// Provider and derived usage facts.
    pub usage: Usage,
    /// Engine handles and model identity.
    pub engine: EngineProjection,
    /// Worktree and diff evidence.
    pub workspace: WorkspaceProjection,
    /// Last parent input request, when blocked.
    pub pending_input: Option<Value>,
}

impl Projection {
    /// Creates the pre-event projection for a validated task.
    pub fn initial(task_id: String, request: &TaskRequest, created_at: String) -> Self {
        Self {
            version: PROJECTION_VERSION,
            task_id,
            attempt: 1,
            status: TaskStatus::Queued,
            phase: Phase::Starting,
            event_seq: 0,
            plan: Vec::new(),
            active_item: None,
            summary: String::new(),
            created_at: created_at.clone(),
            last_activity_at: created_at,
            usage: Usage::default(),
            engine: EngineProjection {
                kind: request.engine.kind.clone(),
                requested_model: request.engine.model.clone(),
                observed_models: vec![request.engine.model.clone()],
                thread_id: None,
                session_id: None,
                turn_id: None,
            },
            workspace: WorkspaceProjection::default(),
            pending_input: None,
        }
    }
}

/// Applies exactly one gap-free event without I/O.
pub fn apply(current: &Projection, event: &Event) -> Result<Projection> {
    if event.task_id != current.task_id || event.attempt != current.attempt {
        return Err(Error::new(
            ErrorKind::InvalidInput,
            "event identity does not match projection",
        ));
    }
    let expected = current
        .event_seq
        .checked_add(1)
        .ok_or_else(|| Error::new(ErrorKind::InvalidInput, "event sequence exhausted"))?;
    if event.seq != expected {
        return Err(Error::new(
            ErrorKind::InvalidInput,
            "event sequence is not contiguous",
        ));
    }
    if current.status.is_terminal() {
        return Err(Error::new(
            ErrorKind::InvalidInput,
            "terminal task cannot transition",
        ));
    }
    let mut next = current.clone();
    next.event_seq = event.seq;
    next.last_activity_at.clone_from(&event.observed_at);
    match event.kind.as_str() {
        "engine.starting" => next.status = TaskStatus::Starting,
        "workspace.prepared" => apply_workspace(&mut next, &event.data),
        "engine.bound" => apply_engine_bound(&mut next, &event.data),
        "turn.started" => {
            next.status = TaskStatus::Running;
            next.phase = Phase::Acting;
            next.engine.turn_id = string_field(&event.data, "turn_id");
        }
        "plan.updated" => apply_plan(&mut next, &event.data)?,
        "item.started" => {
            next.active_item = event.data.get("item").cloned();
            if event.data.get("tool").and_then(Value::as_bool) == Some(true) {
                next.usage.tool_calls =
                    next.usage.tool_calls.checked_add(1).ok_or_else(|| {
                        Error::new(ErrorKind::InvalidInput, "tool counter exhausted")
                    })?;
            }
        }
        "item.completed" => {
            next.active_item = None;
            if let Some(summary) = string_field(&event.data, "summary") {
                next.summary = summary;
            }
        }
        "usage.updated" => apply_usage(&mut next, &event.data),
        "model.rerouted" => apply_reroute(&mut next, &event.data),
        "workspace.diff.updated" => apply_diff(&mut next, &event.data),
        "input.required" => {
            next.status = TaskStatus::InputRequired;
            next.phase = Phase::Waiting;
            next.pending_input = Some(event.data.clone());
        }
        "input.resolved" => {
            next.status = TaskStatus::Running;
            next.phase = Phase::Acting;
            next.pending_input = None;
        }
        "task.stalled" => {
            next.status = TaskStatus::Stalled;
            next.phase = Phase::Waiting;
        }
        "task.resumed" => {
            next.status = TaskStatus::Running;
            next.phase = Phase::Acting;
        }
        "turn.completed" => apply_turn_completed(&mut next, &event.data),
        "task.failed" | "engine.protocol_error" => next.status = TaskStatus::Failed,
        "task.cancelled" => next.status = TaskStatus::Cancelled,
        "task.escalated" | "budget.exceeded" => next.status = TaskStatus::Escalated,
        "task.accepted" | "item.progress" | "engine.unknown" | "engine.stderr"
        | "task.heartbeat" => {}
        other => {
            return Err(Error::new(
                ErrorKind::InvalidInput,
                format!("unknown normalized event {other}"),
            ));
        }
    }
    if next.status.is_terminal() {
        next.phase = Phase::Delivering;
    }
    Ok(next)
}

fn apply_workspace(next: &mut Projection, data: &Value) {
    if let Some(path) = string_field(data, "path") {
        next.workspace.path = path;
    }
    if let Some(base) = string_field(data, "base_revision") {
        next.workspace.base_revision = base;
    }
}

fn apply_engine_bound(next: &mut Projection, data: &Value) {
    next.engine.thread_id = string_field(data, "thread_id");
    next.engine.session_id = string_field(data, "session_id");
}

fn apply_plan(next: &mut Projection, data: &Value) -> Result<()> {
    let value = match data.get("plan") {
        Some(value) => value.clone(),
        None => Value::Array(Vec::new()),
    };
    next.plan = serde_json::from_value(value)?;
    Ok(())
}

fn apply_usage(next: &mut Projection, data: &Value) {
    set_optional_u64(&mut next.usage.input_tokens, data, "input_tokens");
    set_optional_u64(
        &mut next.usage.cached_input_tokens,
        data,
        "cached_input_tokens",
    );
    set_optional_u64(&mut next.usage.output_tokens, data, "output_tokens");
    set_optional_u64(&mut next.usage.reasoning_tokens, data, "reasoning_tokens");
}

fn apply_reroute(next: &mut Projection, data: &Value) {
    if let Some(model) = string_field(data, "to")
        && next.engine.observed_models.last() != Some(&model)
    {
        next.engine.observed_models.push(model);
    }
}

fn apply_diff(next: &mut Projection, data: &Value) {
    next.workspace.diff_hash = string_field(data, "diff_hash");
    if let Some(count) = data.get("changed_files").and_then(Value::as_u64) {
        next.workspace.changed_files = count;
    }
}

fn apply_turn_completed(next: &mut Projection, data: &Value) {
    next.status = match data.get("status").and_then(Value::as_str) {
        Some("completed") => TaskStatus::Completed,
        Some("cancelled" | "interrupted") => TaskStatus::Cancelled,
        Some("failed") => TaskStatus::Failed,
        _ => TaskStatus::Escalated,
    };
}

fn set_optional_u64(target: &mut Option<u64>, data: &Value, field: &str) {
    if let Some(value) = data.get(field).and_then(Value::as_u64) {
        *target = Some(value);
    }
}

fn string_field(value: &Value, field: &str) -> Option<String> {
    value.get(field).and_then(Value::as_str).map(str::to_owned)
}

#[cfg(test)]
mod tests {
    use super::{Projection, apply};
    use crate::protocol::{Event, EventSource, TaskRequest};

    #[test]
    fn terminal_projection_cannot_resume() -> Result<(), Box<dyn std::error::Error>> {
        let request: TaskRequest =
            serde_json::from_str(include_str!("../tests/fixtures/task-request.json"))?;
        let current = Projection::initial("task".to_owned(), &request, "now".to_owned());
        let terminal = apply(
            &current,
            &event(
                1,
                "turn.completed",
                serde_json::json!({"status":"completed"}),
            ),
        )?;
        assert!(apply(&terminal, &event(2, "task.resumed", serde_json::json!({}))).is_err());
        Ok(())
    }

    fn event(seq: u64, kind: &str, data: serde_json::Value) -> Event {
        Event {
            protocol_version: "0.1".to_owned(),
            task_id: "task".to_owned(),
            attempt: 1,
            seq,
            kind: kind.to_owned(),
            observed_at: "now".to_owned(),
            data,
            source: Some(EventSource {
                engine: "fake".to_owned(),
                method: "fake".to_owned(),
                thread_id: None,
                turn_id: None,
                item_id: None,
                payload_hash: "hash".to_owned(),
            }),
        }
    }
}
