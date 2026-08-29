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
        capsule: crate::capsule::receipt_evidence(request),
        final_event_seq: projection.event_seq,
        completed_at: now()?,
    })
}

pub(crate) fn build_failure_receipt(
    projection: &Projection,
    request: &TaskRequest,
) -> Result<Receipt> {
    let mut usage: Usage = projection.usage.clone();
    usage.wall_ms = 0;
    Ok(Receipt {
        protocol_version: PROTOCOL_VERSION.to_owned(),
        receipt_id: new_id("rcp")?,
        task_id: projection.task_id.clone(),
        attempt: projection.attempt,
        status: ReceiptStatus::Failed,
        summary: projection.summary.clone(),
        artifacts: Vec::new(),
        verification: Vec::new(),
        verification_waiver: Some(
            "The worker failed before workspace verification completed.".to_owned(),
        ),
        usage,
        engine: ReceiptEngine {
            kind: request.engine.kind.clone(),
            requested_model: request.engine.model.clone(),
            observed_models: projection.engine.observed_models.clone(),
            version: None,
        },
        capsule: crate::capsule::receipt_evidence(request),
        final_event_seq: projection.event_seq,
        completed_at: now()?,
    })
}

pub(crate) fn build_cancelled_receipt(
    projection: &Projection,
    request: &TaskRequest,
) -> Result<Receipt> {
    let mut receipt = build_failure_receipt(projection, request)?;
    receipt.status = ReceiptStatus::Cancelled;
    receipt.summary = if projection.summary.is_empty() {
        "Task cancelled by the parent harness.".to_owned()
    } else {
        projection.summary.clone()
    };
    receipt.verification_waiver =
        Some("Cancellation ended the worker before acceptance verification completed.".to_owned());
    Ok(receipt)
}

pub(crate) fn build_escalated_receipt(
    projection: &Projection,
    request: &TaskRequest,
) -> Result<Receipt> {
    let mut receipt = build_failure_receipt(projection, request)?;
    receipt.status = ReceiptStatus::Escalated;
    receipt.summary = if projection.summary.is_empty() {
        "Execution outcome is uncertain after service recovery.".to_owned()
    } else {
        projection.summary.clone()
    };
    receipt.verification_waiver = Some(
        "Spewer did not retry work whose prior side effects could not be proven absent.".to_owned(),
    );
    Ok(receipt)
}
