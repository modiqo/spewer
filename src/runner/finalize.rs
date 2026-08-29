use super::{EngineRunMeta, RunResult};
use crate::codex::NormalizedEvent;
use crate::error::{Error, ErrorKind, Result};
use crate::journal::TaskJournal;
use crate::protocol::{Receipt, TaskHandle, TaskRequest};
use crate::receipt::build_receipt;
use crate::workspace::{Workspace, WorkspaceEvidence};
use serde_json::Value;
use tokio::time::Instant;

pub(crate) async fn finish_terminal(
    mut task: TaskJournal<'_>,
    handle: TaskHandle,
    workspace: Workspace,
    request: &TaskRequest,
    evidence: WorkspaceEvidence,
    mut terminal: NormalizedEvent,
    meta: EngineRunMeta,
) -> Result<RunResult> {
    let elapsed = elapsed_ms(meta.started)?;
    if let Some(data) = terminal.data.as_object_mut() {
        data.insert("wall_ms".to_owned(), Value::from(elapsed));
    }
    let Some(database) = task.database else {
        task.append_normalized(terminal).await?;
        return finish(task, handle, workspace, request, evidence, meta).await;
    };
    let projection = task.preview_normalized(&terminal)?;
    let mut receipt = build_receipt(&projection, request, evidence, elapsed, meta.version)?;
    price_and_enforce(request, &mut receipt).await?;
    let finalized = database
        .finalize(
            crate::store::EventInput {
                task_id: projection.task_id.clone(),
                attempt: projection.attempt,
                kind: terminal.kind,
                data: terminal.data,
                source: Some(terminal.source),
                source_key: Some(terminal.source_key),
                observed_at: terminal.observed_at,
            },
            receipt.clone(),
            request.callback.mode.clone(),
        )
        .await?;
    task.projection = finalized.append.projection;
    if finalized.append.inserted {
        task.events.push(finalized.append.event);
    }
    Ok(RunResult {
        handle,
        events: task.events,
        projection: task.projection,
        receipt,
        callback: Some(finalized.message),
        workspace,
    })
}

pub(crate) async fn finish(
    task: TaskJournal<'_>,
    handle: TaskHandle,
    workspace: Workspace,
    request: &TaskRequest,
    evidence: WorkspaceEvidence,
    meta: EngineRunMeta,
) -> Result<RunResult> {
    let elapsed = elapsed_ms(meta.started)?;
    let mut receipt = build_receipt(&task.projection, request, evidence, elapsed, meta.version)?;
    price_and_enforce(request, &mut receipt).await?;
    let callback = match task.database {
        Some(database) => Some(
            database
                .commit_receipt(receipt.clone(), request.callback.mode.clone())
                .await?,
        ),
        None => None,
    };
    Ok(RunResult {
        handle,
        events: task.events,
        projection: task.projection,
        receipt,
        callback,
        workspace,
    })
}

fn elapsed_ms(started: Instant) -> Result<u64> {
    u64::try_from(started.elapsed().as_millis())
        .map_err(|error| Error::new(ErrorKind::InvalidInput, error.to_string()))
}

async fn price_and_enforce(request: &TaskRequest, receipt: &mut Receipt) -> Result<()> {
    crate::telemetry::price_from_environment(receipt).await?;
    if receipt
        .usage
        .actual_cost_usd
        .is_some_and(|cost| cost > request.budgets.cost_usd)
    {
        receipt.status = crate::protocol::ReceiptStatus::Escalated;
        "Cost budget exceeded after final provider usage reconciliation."
            .clone_into(&mut receipt.summary);
    }
    Ok(())
}
