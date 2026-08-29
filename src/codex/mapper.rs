use super::CodexMessage;
use crate::error::{Error, ErrorKind, Result};
use crate::protocol::EventSource;
use crate::util::{now, sha256};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::HashMap;

/// One engine-neutral event before durable task sequencing.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct NormalizedEvent {
    /// Stable normalized event type.
    pub kind: String,
    /// Normalized event data.
    pub data: Value,
    /// Engine source provenance.
    pub source: EventSource,
    /// Deterministic source deduplication key.
    pub source_key: String,
    /// RFC 3339 observation time.
    pub observed_at: String,
}

/// Stateful Codex event normalizer with deterministic duplicate ordinals.
#[derive(Debug, Default)]
pub struct Normalizer {
    occurrences: HashMap<String, u64>,
}

impl Normalizer {
    /// Normalizes one App Server message without exposing native types to core code.
    pub fn normalize(&mut self, message: CodexMessage) -> Result<NormalizedEvent> {
        let (method, params, kind, data) = match message {
            CodexMessage::Notification { method, params } => {
                let (kind, data) = map_notification(&method, &params);
                (method, params, kind, data)
            }
            CodexMessage::ServerRequest { id, method, params } => {
                let data = json!({"request_id": id, "method": method, "request": params});
                (method, params, "input.required".to_owned(), data)
            }
            CodexMessage::Malformed { line, error } => {
                let params = json!({"line_hash": sha256(line.as_bytes())?, "error": error});
                (
                    "malformed".to_owned(),
                    params.clone(),
                    "engine.protocol_error".to_owned(),
                    params,
                )
            }
            CodexMessage::Stderr(line) => {
                let params = json!({"line_hash": sha256(line.as_bytes())?});
                (
                    "stderr".to_owned(),
                    params.clone(),
                    "engine.stderr".to_owned(),
                    params,
                )
            }
            CodexMessage::Exited(code) => {
                let params = json!({"exit_code": code});
                (
                    "process/exited".to_owned(),
                    params.clone(),
                    "task.failed".to_owned(),
                    params,
                )
            }
        };
        let payload = serde_json::to_vec(&params)?;
        let payload_hash = sha256(&payload)?;
        let thread_id = id_at(&params, &["threadId"]).or_else(|| id_at(&params, &["thread", "id"]));
        let turn_id = id_at(&params, &["turnId"]).or_else(|| id_at(&params, &["turn", "id"]));
        let item_id = id_at(&params, &["itemId"]).or_else(|| id_at(&params, &["item", "id"]));
        let fingerprint = format!(
            "{method}:{payload_hash}:{}:{}:{}",
            option_text(thread_id.as_deref()),
            option_text(turn_id.as_deref()),
            option_text(item_id.as_deref())
        );
        let previous = match self.occurrences.get(&fingerprint) {
            Some(value) => *value,
            None => 0,
        };
        let occurrence = previous
            .checked_add(1)
            .ok_or_else(|| Error::new(ErrorKind::EngineProtocol, "source occurrence exhausted"))?;
        self.occurrences.insert(fingerprint.clone(), occurrence);
        Ok(NormalizedEvent {
            kind,
            data,
            source: EventSource {
                engine: "codex-app-server".to_owned(),
                method,
                thread_id,
                turn_id,
                item_id,
                payload_hash,
            },
            source_key: format!("{fingerprint}:{occurrence}"),
            observed_at: now()?,
        })
    }
}

fn map_notification(method: &str, params: &Value) -> (String, Value) {
    let mapped = match method {
        "thread/started" => (
            "engine.bound",
            json!({
                "thread_id": id_at(params, &["thread", "id"]),
                "session_id": id_at(params, &["thread", "sessionId"])
            }),
        ),
        "turn/started" => (
            "turn.started",
            json!({"turn_id": id_at(params, &["turn", "id"])}),
        ),
        "turn/plan/updated" => ("plan.updated", json!({"plan": normalize_plan(params)})),
        "item/started" => (
            "item.started",
            json!({
                "item": item_metadata(params.get("item")),
                "tool": is_tool_item(params.get("item"))
            }),
        ),
        "item/completed" => (
            "item.completed",
            json!({
                "item": item_metadata(params.get("item")),
                "summary": agent_summary(params.get("item"))
            }),
        ),
        "turn/diff/updated" => (
            "workspace.diff.updated",
            json!({"diff_bytes": params.get("diff").and_then(Value::as_str).map(str::len)}),
        ),
        "thread/tokenUsage/updated" => ("usage.updated", normalize_usage(params)),
        "model/rerouted" => (
            "model.rerouted",
            json!({"from": params.get("fromModel"), "to": params.get("toModel"), "reason": params.get("reason")}),
        ),
        "turn/completed" => (
            "turn.completed",
            json!({
                "status": id_at(params, &["turn", "status"]),
                "error": params.pointer("/turn/error")
            }),
        ),
        method if is_progress_method(method) => (
            "item.progress",
            json!({"method": method, "delta_bytes": delta_bytes(params)}),
        ),
        _ => ("engine.unknown", json!({"method": method})),
    };
    (mapped.0.to_owned(), mapped.1)
}

fn normalize_plan(params: &Value) -> Vec<Value> {
    let Some(plan) = params.get("plan").and_then(Value::as_array) else {
        return Vec::new();
    };
    let mut normalized = Vec::with_capacity(plan.len());
    for entry in plan {
        let step = entry
            .get("step")
            .and_then(Value::as_str)
            .map_or("", |value| value);
        let status = match entry.get("status").and_then(Value::as_str) {
            Some("inProgress") => "in_progress",
            Some(value) => value,
            None => "pending",
        };
        normalized.push(json!({"step": step, "status": status}));
    }
    normalized
}

fn item_metadata(item: Option<&Value>) -> Value {
    json!({
        "id": item.and_then(|value| value.get("id")),
        "type": item.and_then(|value| value.get("type")),
        "status": item.and_then(|value| value.get("status"))
    })
}

fn normalize_usage(params: &Value) -> Value {
    let total = params.pointer("/tokenUsage/total");
    json!({
        "input_tokens": total.and_then(|value| value.get("inputTokens")),
        "cached_input_tokens": total.and_then(|value| value.get("cachedInputTokens")),
        "output_tokens": total.and_then(|value| value.get("outputTokens")),
        "reasoning_tokens": total.and_then(|value| value.get("reasoningOutputTokens"))
    })
}

fn is_progress_method(method: &str) -> bool {
    method.ends_with("/delta")
        || method.ends_with("/outputDelta")
        || method.contains("progress")
        || method == "turn/diff/updated"
}

fn is_tool_item(item: Option<&Value>) -> bool {
    matches!(
        item.and_then(|value| value.get("type"))
            .and_then(Value::as_str),
        Some(
            "commandExecution"
                | "fileChange"
                | "mcpToolCall"
                | "dynamicToolCall"
                | "webSearch"
                | "collabAgentToolCall"
        )
    )
}

fn agent_summary(item: Option<&Value>) -> Option<&str> {
    let item = item?;
    if item.get("type").and_then(Value::as_str) == Some("agentMessage") {
        return item.get("text").and_then(Value::as_str);
    }
    None
}

fn delta_bytes(params: &Value) -> u64 {
    params
        .get("delta")
        .and_then(Value::as_str)
        .and_then(|value| u64::try_from(value.len()).ok())
        .map_or(0, |value| value)
}

fn option_text(value: Option<&str>) -> &str {
    match value {
        Some(value) => value,
        None => "-",
    }
}

fn id_at(value: &Value, path: &[&str]) -> Option<String> {
    let mut current = value;
    for component in path {
        current = current.get(*component)?;
    }
    current.as_str().map(str::to_owned)
}

#[cfg(test)]
mod tests {
    use super::Normalizer;
    use crate::codex::CodexMessage;
    use serde_json::json;

    #[test]
    fn maps_plan_and_preserves_source_identity() -> Result<(), Box<dyn std::error::Error>> {
        let mut normalizer = Normalizer::default();
        let event = normalizer.normalize(CodexMessage::Notification {
            method: "turn/plan/updated".to_owned(),
            params: json!({"threadId":"t","turnId":"u","plan":[{"step":"work","status":"inProgress"}]}),
        })?;
        assert_eq!(event.kind, "plan.updated");
        assert_eq!(
            event.data.pointer("/plan/0/status"),
            Some(&json!("in_progress"))
        );
        assert!(event.source_key.ends_with(":1"));
        Ok(())
    }

    #[test]
    fn item_events_do_not_persist_native_prompt_bodies() -> Result<(), Box<dyn std::error::Error>> {
        let mut normalizer = Normalizer::default();
        let event = normalizer.normalize(CodexMessage::Notification {
            method: "item/started".to_owned(),
            params: json!({
                "item":{"id":"item-one","type":"userMessage","text":"unique-secret-body"}
            }),
        })?;
        let encoded = serde_json::to_string(&event.data)?;
        assert!(!encoded.contains("unique-secret-body"));
        assert_eq!(event.data.pointer("/item/id"), Some(&json!("item-one")));
        Ok(())
    }
}
