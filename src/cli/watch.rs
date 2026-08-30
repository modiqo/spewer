//! Human-readable, secret-safe durable task tracing.

use crate::capsule::{CapsuleEvidence, CapsuleKind};
use crate::error::Result;
use crate::protocol::{Event, TaskRequest, TaskStatus};
use crate::store::Database;
use serde_json::Value;
use std::fmt::Write as _;
use std::time::Duration;

pub(super) async fn run(task_id: String, after: u64) -> Result<()> {
    let database = Database::open(Database::default_path()?).await?;
    let request = database.request(task_id.clone()).await?;
    print_header(&task_id, &request);
    let mut cursor = after;
    loop {
        let observation = database.observe(task_id.clone(), cursor).await?;
        for event in &observation.events {
            if let Some(line) = render_event(event) {
                println!("{line}");
            }
        }
        cursor = observation.next_cursor;
        if observation.projection.status.is_terminal() {
            println!(
                "done status={} cursor={cursor}",
                status_name(observation.projection.status)
            );
            break;
        }
        tokio::time::sleep(Duration::from_millis(
            observation.poll_after_ms.clamp(100, 2_000),
        ))
        .await;
    }
    database.close().await
}

fn print_header(task_id: &str, request: &TaskRequest) {
    println!(
        "watch task={} engine={} model={}",
        safe_text(task_id),
        safe_text(&request.engine.kind),
        safe_text(&request.engine.model)
    );
    match crate::capsule::receipt_evidence(request) {
        Some(evidence) => println!("{}", capsule_line(&evidence)),
        None => println!("capsule unbound"),
    }
}

fn capsule_line(evidence: &CapsuleEvidence) -> String {
    let kind = match evidence.kind {
        CapsuleKind::Generic => "generic",
        CapsuleKind::Specialized => "specialized",
    };
    let mut line = format!("capsule id={} kind={kind}", safe_text(&evidence.id));
    if let Some(skill) = &evidence.skill {
        let _written = write!(
            line,
            " skill={} revision={} digest={}",
            safe_text(&skill.name),
            safe_text(&skill.revision),
            safe_text(
                skill
                    .digest
                    .get(..12)
                    .map_or(skill.digest.as_str(), |value| value)
            )
        );
    }
    line
}

fn render_event(event: &Event) -> Option<String> {
    let detail = match event.kind.as_str() {
        "task.accepted" => "accepted".to_owned(),
        "turn.leased" => "worker leased".to_owned(),
        "workspace.prepared" => "workspace ready".to_owned(),
        "engine.starting" => "engine starting".to_owned(),
        "engine.bound" => "engine ready".to_owned(),
        "turn.started" => "model started".to_owned(),
        "plan.updated" => format!("plan updated steps={}", array_len(&event.data, "plan")),
        "item.started" => render_item("started", &event.data)?,
        "item.completed" => render_item("completed", &event.data)?,
        "task.heartbeat" => format!(
            "model active elapsed_ms={}",
            number(&event.data, "elapsed_ms").map_or(0, |value| value)
        ),
        "usage.updated" => render_usage(&event.data),
        "model.rerouted" => format!(
            "model rerouted to={}",
            text_at(&event.data, "/to").map_or("not-reported".to_owned(), safe_text)
        ),
        "workspace.diff.updated" => format!(
            "workspace checked changed_files={}",
            number(&event.data, "changed_files").map_or(0, |value| value)
        ),
        "input.required" => "waiting for input".to_owned(),
        "input.resolved" => "input received".to_owned(),
        "task.stalled" => "worker stalled".to_owned(),
        "task.resumed" => "worker resumed".to_owned(),
        "budget.exceeded" => "budget exceeded".to_owned(),
        "task.cancelled" => "cancelled".to_owned(),
        "task.failed" | "engine.protocol_error" => "failed".to_owned(),
        "task.escalated" => "escalated".to_owned(),
        "turn.completed" => "model completed".to_owned(),
        _ => return None,
    };
    Some(format!("{:04} {detail}", event.seq))
}

fn render_item(action: &str, data: &Value) -> Option<String> {
    let Some(item) = data.get("item") else {
        if let Some(tool) = data.get("tool").and_then(Value::as_str) {
            return Some(format!("tool {action} {}", safe_text(tool)));
        }
        return data
            .get("summary")
            .and_then(Value::as_str)
            .map(|_| format!("response {action}"));
    };
    let kind = item.get("type")?.as_str()?;
    let tool = data.get("tool").and_then(Value::as_bool) == Some(true)
        || matches!(
            kind,
            "commandExecution"
                | "fileChange"
                | "mcpToolCall"
                | "dynamicToolCall"
                | "webSearch"
                | "collabAgentToolCall"
                | "tool_call"
        );
    if tool {
        return Some(format!("tool {action} {}", item_label(item, kind)));
    }
    match kind {
        "agentMessage" | "agent_message" => Some(format!("response {action}")),
        "reasoning" => Some(format!("reasoning {action}")),
        _ => None,
    }
}

fn item_label(item: &Value, kind: &str) -> String {
    let fields = [
        "command_name",
        "name",
        "plugin_id",
        "script_path",
        "server",
        "namespace",
        "tool",
    ];
    let labels = fields
        .into_iter()
        .filter_map(|field| item.get(field).and_then(Value::as_str))
        .map(safe_text)
        .collect::<Vec<_>>();
    if labels.is_empty() {
        safe_text(kind)
    } else {
        format!("{} {}", safe_text(kind), labels.join("/"))
    }
}

fn render_usage(data: &Value) -> String {
    format!(
        "usage input={} cached={} output={} reasoning={}",
        report_number(data, "input_tokens"),
        report_number(data, "cached_input_tokens"),
        report_number(data, "output_tokens"),
        report_number(data, "reasoning_tokens")
    )
}

fn report_number(data: &Value, field: &str) -> String {
    number(data, field).map_or("not-reported".to_owned(), |value| value.to_string())
}

fn number(data: &Value, field: &str) -> Option<u64> {
    data.get(field).and_then(Value::as_u64)
}

fn array_len(data: &Value, field: &str) -> usize {
    data.get(field)
        .and_then(Value::as_array)
        .map_or(0, Vec::len)
}

fn text_at<'a>(data: &'a Value, pointer: &str) -> Option<&'a str> {
    data.pointer(pointer).and_then(Value::as_str)
}

fn safe_text(value: &str) -> String {
    value
        .chars()
        .take(96)
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '.' | '_' | '-' | ':' | '/')
            {
                character
            } else {
                '?'
            }
        })
        .collect()
}

fn status_name(status: TaskStatus) -> &'static str {
    match status {
        TaskStatus::Queued => "queued",
        TaskStatus::Starting => "starting",
        TaskStatus::Running => "running",
        TaskStatus::InputRequired => "input_required",
        TaskStatus::Stalled => "stalled",
        TaskStatus::Completed => "completed",
        TaskStatus::Failed => "failed",
        TaskStatus::Cancelled => "cancelled",
        TaskStatus::Escalated => "escalated",
    }
}

#[cfg(test)]
mod tests {
    use super::render_event;
    use crate::protocol::{Event, PROTOCOL_VERSION};
    use serde_json::json;

    fn event(kind: &str, data: serde_json::Value) -> Event {
        Event {
            protocol_version: PROTOCOL_VERSION.to_owned(),
            task_id: "tsk_test".to_owned(),
            attempt: 1,
            seq: 7,
            kind: kind.to_owned(),
            observed_at: "2026-08-29T00:00:00Z".to_owned(),
            data,
            source: None,
        }
    }

    #[test]
    fn renders_safe_tool_identity_without_arguments() -> Result<(), Box<dyn std::error::Error>> {
        let line = render_event(&event(
            "item.started",
            json!({
                "tool": true,
                "item": {
                    "type": "commandExecution",
                    "command_name": "play-machine",
                    "arguments": "secret-token"
                }
            }),
        ))
        .ok_or("tool line")?;
        assert!(line.contains("play-machine"));
        assert!(!line.contains("secret-token"));
        Ok(())
    }

    #[test]
    fn filters_provider_noise_and_reports_ollama_heartbeat() {
        assert!(render_event(&event("item.progress", json!({"delta_bytes": 42}))).is_none());
        assert_eq!(
            render_event(&event(
                "task.heartbeat",
                json!({"activity":"model_active","elapsed_ms":2000})
            )),
            Some("0007 model active elapsed_ms=2000".to_owned())
        );
    }
}
