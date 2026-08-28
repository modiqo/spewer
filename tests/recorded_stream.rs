//! Recorded Codex stream contract test.

use spewer::codex::{CodexMessage, Normalizer};
use spewer::protocol::{Event, TaskRequest};
use spewer::reducer::{Projection, apply};

#[test]
fn happy_stream_reaches_a_terminal_projection() -> Result<(), Box<dyn std::error::Error>> {
    let request: TaskRequest = serde_json::from_str(include_str!("fixtures/task-request.json"))?;
    let mut projection = Projection::initial("task".to_owned(), &request, "start".to_owned());
    let mut normalizer = Normalizer::default();
    let stream = include_str!("fixtures/codex-happy.jsonl");
    for line in stream.lines().filter(|line| !line.is_empty()) {
        let source: serde_json::Value = serde_json::from_str(line)?;
        let method = source
            .get("method")
            .and_then(serde_json::Value::as_str)
            .ok_or("fixture method missing")?
            .to_owned();
        let params = source
            .get("params")
            .cloned()
            .ok_or("fixture params missing")?;
        let mapped = normalizer.normalize(CodexMessage::Notification { method, params })?;
        let seq = projection
            .event_seq
            .checked_add(1)
            .ok_or("sequence overflow")?;
        let event = Event {
            protocol_version: "0.1".to_owned(),
            task_id: "task".to_owned(),
            attempt: 1,
            seq,
            kind: mapped.kind,
            observed_at: mapped.observed_at,
            data: mapped.data,
            source: Some(mapped.source),
        };
        projection = apply(&projection, &event)?;
    }
    assert!(projection.status.is_terminal());
    assert_eq!(projection.summary, "Created the requested result.");
    assert_eq!(projection.usage.output_tokens, Some(20));
    assert_eq!(projection.usage.tool_calls, 1);
    Ok(())
}
