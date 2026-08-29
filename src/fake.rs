//! Deterministic second engine for contract and crash testing.

use crate::engine::{EngineAdapter, EngineCapabilities, EngineEvent, negotiate};
use crate::error::{Error, ErrorKind, Result};
use crate::protocol::TaskRequest;
use serde::{Deserialize, Serialize};
use serde_json::json;

/// Script controls the fake engine without timers or nondeterminism.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct FakeScript {
    /// Emit a parent input request instead of completion.
    pub pause_for_input: bool,
    /// Fail after emitting the plan.
    pub fail_after_plan: bool,
    /// Duplicate every source event once.
    pub duplicate_sources: bool,
}

/// Engine adapter used to prove the public boundary is replaceable.
#[derive(Clone, Debug)]
pub struct FakeEngine {
    capabilities: EngineCapabilities,
    script: FakeScript,
}

impl FakeEngine {
    /// Creates a deterministic fake model family.
    pub fn new(script: FakeScript) -> Self {
        Self {
            capabilities: EngineCapabilities {
                kind: "fake".to_owned(),
                models: vec!["fake-local".to_owned()],
                resumable: true,
                usage: true,
            },
            script,
        }
    }

    fn scripted(&self, request: &TaskRequest) -> Result<Vec<EngineEvent>> {
        negotiate(&self.capabilities, request, false)?;
        let mut events = vec![
            event(
                "thread/started",
                "engine.bound",
                json!({"thread_id":"fake-thread","session_id":"fake-session"}),
                1,
            ),
            event(
                "turn/started",
                "turn.started",
                json!({"turn_id":"fake-turn"}),
                2,
            ),
            event(
                "turn/plan/updated",
                "plan.updated",
                json!({"plan":[{"step":"execute fixture","status":"in_progress"}]}),
                3,
            ),
        ];
        if self.script.fail_after_plan {
            events.push(event(
                "engine/failure",
                "task.failed",
                json!({"reason":"scripted"}),
                4,
            ));
        } else if self.script.pause_for_input {
            events.push(event(
                "input/request",
                "input.required",
                json!({"prompt":"fixture input"}),
                4,
            ));
        } else {
            events.extend([
                event("item/started", "item.started", json!({"tool":true}), 4),
                event(
                    "item/completed",
                    "item.completed",
                    json!({"summary":"fake task complete"}),
                    5,
                ),
                event(
                    "usage",
                    "usage.updated",
                    json!({"input_tokens":100,"output_tokens":20}),
                    6,
                ),
                event(
                    "turn/completed",
                    "turn.completed",
                    json!({"status":"completed"}),
                    7,
                ),
            ]);
        }
        if self.script.duplicate_sources {
            let duplicate = events.clone();
            events.extend(duplicate);
        }
        Ok(events)
    }
}

impl EngineAdapter for FakeEngine {
    fn capabilities(&self) -> &EngineCapabilities {
        &self.capabilities
    }

    async fn execute(&mut self, request: &TaskRequest) -> Result<Vec<EngineEvent>> {
        self.scripted(request)
    }
}

fn event(method: &str, kind: &str, data: serde_json::Value, ordinal: u64) -> EngineEvent {
    EngineEvent {
        method: method.to_owned(),
        kind: kind.to_owned(),
        data,
        source_key: format!("fake:{method}:{ordinal}"),
    }
}

/// Validates that an event stream terminates or pauses explicitly.
pub fn validate_stream(events: &[EngineEvent]) -> Result<()> {
    let terminal = events.iter().any(|event| {
        matches!(
            event.kind.as_str(),
            "turn.completed" | "task.failed" | "input.required"
        )
    });
    if !terminal {
        return Err(Error::new(
            ErrorKind::EngineProtocol,
            "engine stream ended without a terminal or pause event",
        ));
    }
    Ok(())
}
