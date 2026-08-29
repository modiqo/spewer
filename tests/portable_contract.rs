//! Pareto, parent-handoff, and second-engine conformance tests.

use spewer::delivery::OutboxMessage;
use spewer::engine::{EngineAdapter, negotiate};
use spewer::fake::{FakeEngine, FakeScript, validate_stream};
use spewer::parent::{Handoff, ParentCursor};
use spewer::protocol::{
    Event, EventSource, Receipt, ReceiptEngine, ReceiptStatus, TaskRequest, Usage,
};
use spewer::reducer::{Projection, apply};
use spewer::telemetry::{ModelPrice, PriceConfig, RunExport, comparable, pareto_points};
use std::collections::{BTreeMap, HashSet};

#[tokio::test(flavor = "current_thread")]
async fn fake_engine_passes_common_event_and_budget_contract()
-> Result<(), Box<dyn std::error::Error>> {
    let mut request = request()?;
    request.engine.kind = "fake".to_owned();
    request.engine.model = "fake-local".to_owned();
    let mut engine = FakeEngine::new(FakeScript {
        duplicate_sources: true,
        ..FakeScript::default()
    });
    negotiate(engine.capabilities(), &request, true)?;
    let native = engine.execute(&request).await?;
    validate_stream(&native)?;
    let mut projection = Projection::initial(
        "fake-task".to_owned(),
        &request,
        "2026-08-28T00:00:00Z".to_owned(),
    );
    let mut seen = HashSet::new();
    for source in native {
        if !seen.insert(source.source_key.clone()) {
            continue;
        }
        let seq = projection
            .event_seq
            .checked_add(1)
            .ok_or("sequence overflow")?;
        projection = apply(
            &projection,
            &Event {
                protocol_version: "0.1".to_owned(),
                task_id: "fake-task".to_owned(),
                attempt: 1,
                seq,
                kind: source.kind,
                observed_at: "2026-08-28T00:00:01Z".to_owned(),
                data: source.data,
                source: Some(EventSource {
                    engine: "fake".to_owned(),
                    method: source.method,
                    thread_id: None,
                    turn_id: None,
                    item_id: None,
                    payload_hash: source.source_key,
                }),
            },
        )?;
    }
    assert!(projection.status.is_terminal());
    assert_eq!(projection.usage.input_tokens, Some(100));
    assert_eq!(projection.usage.tool_calls, 1);
    Ok(())
}

#[test]
fn price_hash_quality_inputs_and_task_class_are_traceable() -> Result<(), Box<dyn std::error::Error>>
{
    let mut models = BTreeMap::new();
    models.insert(
        "fake-local".to_owned(),
        ModelPrice {
            input_per_million: 1.0,
            cached_input_per_million: 0.1,
            output_per_million: 2.0,
            reasoning_per_million: 2.0,
        },
    );
    let prices = PriceConfig {
        version: 1,
        source: "contract fixture".to_owned(),
        effective_at: "2026-08-28T00:00:00Z".to_owned(),
        models,
    };
    let mut receipt = receipt();
    receipt.usage.input_tokens = Some(1_000);
    receipt.usage.output_tokens = Some(100);
    prices.price("fake-local", &mut receipt.usage)?;
    assert_eq!(receipt.usage.price_config_hash, Some(prices.hash()?));
    let left = RunExport {
        task_class: "parser-edit".to_owned(),
        receipt: receipt.clone(),
        checks_passed: 2,
        checks_attempted: 2,
    };
    let right = RunExport {
        task_class: "research".to_owned(),
        receipt,
        checks_passed: 1,
        checks_attempted: 1,
    };
    assert!(comparable(&left, &right, false).is_err());
    assert!(left.summary().contains("2/2 checks passed"));
    let mut comparable_right = right;
    comparable_right.task_class.clone_from(&left.task_class);
    let points = pareto_points(&[left, comparable_right], false)?;
    assert_eq!(points.len(), 2);
    assert!(points.iter().all(|point| point.checks_attempted > 0));
    Ok(())
}

#[test]
fn play_style_parent_applies_duplicate_receipt_once() -> Result<(), Box<dyn std::error::Error>> {
    let mut task = request()?;
    task.private_continuation = None;
    let handoff = Handoff::for_play(task)?;
    let receipt = receipt();
    let message = OutboxMessage {
        message_id: "message-one".to_owned(),
        task_id: receipt.task_id.clone(),
        receipt_id: receipt.receipt_id.clone(),
        mode: "poll".to_owned(),
        receipt,
        created_at: "2026-08-28T00:00:02Z".to_owned(),
    };
    let mut cursor = ParentCursor::default();
    let first = cursor.apply(&handoff, message.clone());
    let duplicate = cursor.apply(&handoff, message);
    assert!(first.applied);
    assert!(!duplicate.applied);
    assert_eq!(
        first.private_continuation,
        handoff.task.private_continuation
    );
    let mut invalid = request()?;
    invalid.private_continuation = Some(serde_json::json!({"continuation_id":"opaque"}));
    assert!(Handoff::for_play(invalid).is_err());
    Ok(())
}

fn request() -> Result<TaskRequest, Box<dyn std::error::Error>> {
    Ok(serde_json::from_str(include_str!(
        "fixtures/task-request.json"
    ))?)
}

fn receipt() -> Receipt {
    Receipt {
        protocol_version: "0.1".to_owned(),
        receipt_id: "receipt-one".to_owned(),
        task_id: "fake-task".to_owned(),
        attempt: 1,
        status: ReceiptStatus::Completed,
        summary: "fake task complete".to_owned(),
        artifacts: Vec::new(),
        verification: Vec::new(),
        verification_waiver: Some("contract fixture".to_owned()),
        usage: Usage::default(),
        engine: ReceiptEngine {
            kind: "fake".to_owned(),
            requested_model: "fake-local".to_owned(),
            observed_models: vec!["fake-local".to_owned()],
            version: Some("1".to_owned()),
        },
        final_event_seq: 7,
        completed_at: "2026-08-28T00:00:02Z".to_owned(),
    }
}
