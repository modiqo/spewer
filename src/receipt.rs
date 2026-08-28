use crate::error::Result;
use crate::protocol::{
    PROTOCOL_VERSION, Receipt, ReceiptEngine, ReceiptStatus, TaskRequest, TaskStatus, Usage,
    Verification,
};
use crate::reducer::Projection;
use crate::util::{new_id, now};
use crate::workspace::WorkspaceEvidence;

pub(crate) fn build_receipt(
    projection: &Projection,
    request: &TaskRequest,
    evidence: WorkspaceEvidence,
    wall_ms: u64,
) -> Result<Receipt> {
    let status = match projection.status {
        TaskStatus::Completed => ReceiptStatus::Completed,
        TaskStatus::Cancelled => ReceiptStatus::Cancelled,
        TaskStatus::Escalated | TaskStatus::InputRequired => ReceiptStatus::Escalated,
        _ => ReceiptStatus::Failed,
    };
    let summary = if projection.summary.is_empty() {
        "Worker ended without an agent summary.".to_owned()
    } else {
        projection.summary.clone()
    };
    let mut usage: Usage = projection.usage.clone();
    usage.wall_ms = wall_ms;
    Ok(Receipt {
        protocol_version: PROTOCOL_VERSION.to_owned(),
        receipt_id: new_id("rcp")?,
        task_id: projection.task_id.clone(),
        attempt: projection.attempt,
        status,
        summary,
        artifacts: vec![evidence.artifact],
        verification: vec![Verification {
            command: "workspace path boundary".to_owned(),
            exit_code: Some(0),
            output_sha256: Some(evidence.diff_hash),
            passed: true,
        }],
        verification_waiver: Some(
            "Parent acceptance verification remains required after the bounded worker run."
                .to_owned(),
        ),
        usage,
        engine: ReceiptEngine {
            kind: request.engine.kind.clone(),
            requested_model: request.engine.model.clone(),
            observed_models: projection.engine.observed_models.clone(),
            version: Some("codex-cli 0.150.1".to_owned()),
        },
        final_event_seq: projection.event_seq,
        completed_at: now()?,
    })
}
