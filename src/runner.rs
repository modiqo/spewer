//! One bounded engine run from validated request to receipt.

mod accepted;

pub use accepted::{fail_durable, run_codex_accepted};

use crate::codex::{
    CodexClient, CodexConfig, CodexMessage, NormalizedEvent, Normalizer, thread_params, turn_params,
};
use crate::delivery::OutboxMessage;
use crate::error::{Error, ErrorKind, Result};
use crate::journal::TaskJournal;
use crate::protocol::{Event, PROTOCOL_VERSION, Receipt, TaskHandle, TaskRequest};
use crate::receipt::build_receipt;
use crate::reducer::Projection;
use crate::store::Database;
use crate::util::{new_id, now, required_pointer};
use crate::workspace::{Workspace, WorkspaceEvidence};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::time::Duration;
use tokio::time::Instant;

/// Complete observable result of one in-process run.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct RunResult {
    /// Durable handle created before engine startup.
    pub handle: TaskHandle,
    /// Gap-free normalized events.
    pub events: Vec<Event>,
    /// Final deterministic projection.
    pub projection: Projection,
    /// Terminal receipt.
    pub receipt: Receipt,
    /// Durable callback message for database-backed runs.
    pub callback: Option<OutboxMessage>,
    /// Isolated worktree retained for parent inspection.
    pub workspace: Workspace,
}

pub(crate) struct DriveOutcome {
    pub(crate) evidence: WorkspaceEvidence,
    pub(crate) terminal: Option<NormalizedEvent>,
}

/// Runs one task through Codex App Server and returns a typed receipt.
pub async fn run_codex(request: TaskRequest, config: CodexConfig) -> Result<RunResult> {
    run_codex_inner(request, config, None, None).await
}

/// Runs one task while committing every accepted event to `SQLite`.
pub async fn run_codex_durable(
    request: TaskRequest,
    config: CodexConfig,
    database: &Database,
) -> Result<RunResult> {
    run_codex_inner(request, config, Some(database), None).await
}

pub(super) async fn run_codex_inner(
    request: TaskRequest,
    mut config: CodexConfig,
    database: Option<&Database>,
    accepted_task_id: Option<String>,
) -> Result<RunResult> {
    request.validate()?;
    if request.engine.kind != "codex-app-server" {
        return Err(Error::new(
            ErrorKind::InvalidInput,
            "run_codex requires codex-app-server",
        ));
    }
    for name in &request.permissions.environment_allowlist {
        if !config.inherited_environment.contains(name) {
            config.inherited_environment.push(name.clone());
        }
    }
    let (task_id, handle, mut task) = match accepted_task_id {
        Some(task_id) => accepted::load_accepted_task(&request, database, task_id).await?,
        None => accept_task(&request, database).await?,
    };
    let workspace = Workspace::prepare(&request, &task_id).await?;
    record_workspace(&mut task, &workspace).await?;
    let started = Instant::now();
    let deadline = started
        .checked_add(Duration::from_secs(request.budgets.wall_seconds))
        .ok_or_else(|| Error::new(ErrorKind::InvalidInput, "wall deadline overflow"))?;
    let mut client = CodexClient::connect(config).await?;
    let conversation = start_conversation(&mut client, &request, &workspace, &mut task).await;
    let (thread_id, turn_id) = match conversation {
        Ok(ids) => ids,
        Err(error) => return close_with_error(client, error).await,
    };
    let outcome = drive(
        &mut client,
        &request,
        &workspace,
        &mut task,
        &thread_id,
        &turn_id,
        deadline,
    )
    .await;
    let close_result = client.close().await;
    let driven = match outcome {
        Ok(driven) => driven,
        Err(error) => return combine_close(error, close_result),
    };
    close_result?;
    match driven.terminal {
        Some(terminal) => {
            finish_terminal(
                task,
                handle,
                workspace,
                &request,
                driven.evidence,
                terminal,
                started,
            )
            .await
        }
        None => finish(task, handle, workspace, &request, driven.evidence, started).await,
    }
}

async fn accept_task<'a>(
    request: &TaskRequest,
    database: Option<&'a Database>,
) -> Result<(String, TaskHandle, TaskJournal<'a>)> {
    let task_id = match &request.task_id {
        Some(task_id) => task_id.clone(),
        None => new_id("tsk")?,
    };
    let created_at = now()?;
    if let Some(database) = database {
        let accepted = database
            .accept(request.clone(), task_id.clone(), created_at)
            .await?;
        if !accepted.created {
            return Err(Error::new(
                ErrorKind::InvalidInput,
                format!(
                    "idempotency key already belongs to task {}; use status instead",
                    accepted.handle.task_id
                ),
            ));
        }
        let event = accepted.event.ok_or_else(|| {
            Error::new(ErrorKind::Storage, "created task has no acceptance event")
        })?;
        let journal = TaskJournal {
            projection: accepted.projection,
            events: vec![event],
            database: Some(database),
        };
        return Ok((task_id, accepted.handle, journal));
    }
    let mut task = TaskJournal {
        projection: Projection::initial(task_id.clone(), request, created_at.clone()),
        events: Vec::new(),
        database: None,
    };
    task.append(
        "task.accepted",
        json!({"idempotency_key": request.idempotency_key}),
        None,
        None,
        created_at.clone(),
    )
    .await?;
    let handle = TaskHandle {
        protocol_version: PROTOCOL_VERSION.to_owned(),
        task_id: task_id.clone(),
        status: task.projection.status,
        event_cursor: task.projection.event_seq,
        created_at,
    };
    Ok((task_id, handle, task))
}

async fn record_workspace(task: &mut TaskJournal<'_>, workspace: &Workspace) -> Result<()> {
    task.append(
        "workspace.prepared",
        json!({"path": workspace.path, "base_revision": workspace.base_revision}),
        None,
        None,
        now()?,
    )
    .await?;
    task.append("engine.starting", json!({}), None, None, now()?)
        .await?;
    Ok(())
}

async fn start_conversation(
    client: &mut CodexClient,
    request: &TaskRequest,
    workspace: &Workspace,
    task: &mut TaskJournal<'_>,
) -> Result<(String, String)> {
    ensure_model(client, &request.engine.model).await?;
    let thread = client
        .request("thread/start", thread_params(request, workspace))
        .await?;
    let thread_id = required_pointer(&thread, "/thread/id")?;
    let session_id = required_pointer(&thread, "/thread/sessionId")?;
    task.append(
        "engine.bound",
        json!({"thread_id": thread_id, "session_id": session_id}),
        None,
        None,
        now()?,
    )
    .await?;
    let turn = client
        .request("turn/start", turn_params(request, workspace, &thread_id)?)
        .await?;
    let turn_id = required_pointer(&turn, "/turn/id")?;
    task.append(
        "turn.started",
        json!({"turn_id": turn_id}),
        None,
        None,
        now()?,
    )
    .await?;
    Ok((thread_id, turn_id))
}

pub(crate) async fn drive(
    client: &mut CodexClient,
    request: &TaskRequest,
    workspace: &Workspace,
    task: &mut TaskJournal<'_>,
    thread_id: &str,
    turn_id: &str,
    deadline: Instant,
) -> Result<DriveOutcome> {
    let mut normalizer = Normalizer::default();
    let redactor =
        crate::security::Redactor::from_environment(&request.permissions.environment_allowlist);
    while !task.projection.status.is_terminal() {
        let message = match tokio::time::timeout_at(deadline, client.next_message()).await {
            Ok(Some(message)) => message,
            Ok(None) => CodexMessage::Exited(None),
            Err(_) => {
                let _interrupted = client
                    .request(
                        "turn/interrupt",
                        json!({"threadId": thread_id, "turnId": turn_id}),
                    )
                    .await;
                task.append(
                    "task.cancelled",
                    json!({"reason":"wall budget exceeded"}),
                    None,
                    None,
                    now()?,
                )
                .await?;
                break;
            }
        };
        let mut mapped = normalizer.normalize(message)?;
        redactor.redact(&mut mapped.data);
        if mapped.kind == "turn.completed" {
            let captured = workspace
                .capture(&request.permissions.writable_paths)
                .await?;
            append_diff(task, &captured).await?;
            if let Some(database) = task.database {
                let checkpoint = crate::recovery::checkpoint(
                    &task.projection,
                    &captured,
                    "turn completed",
                    true,
                )?;
                database.save_checkpoint(checkpoint).await?;
            }
            return Ok(DriveOutcome {
                evidence: captured,
                terminal: Some(mapped),
            });
        }
        let input_required = mapped.kind == "input.required";
        task.append_normalized(mapped).await?;
        enforce_runtime_budget(client, request, task, thread_id, turn_id, deadline).await?;
        if input_required {
            task.append(
                "task.escalated",
                json!({"reason":"parent input required"}),
                None,
                None,
                now()?,
            )
            .await?;
        }
    }
    let evidence = workspace
        .capture(&request.permissions.writable_paths)
        .await?;
    Ok(DriveOutcome {
        evidence,
        terminal: None,
    })
}

async fn enforce_runtime_budget(
    client: &mut CodexClient,
    request: &TaskRequest,
    task: &mut TaskJournal<'_>,
    thread_id: &str,
    turn_id: &str,
    deadline: Instant,
) -> Result<()> {
    let wall_limit_ms = request.budgets.wall_seconds.saturating_mul(1_000);
    let remaining_ms = u64::try_from(
        deadline
            .saturating_duration_since(Instant::now())
            .as_millis(),
    )
    .map_or(0, |value| value);
    let elapsed_ms = wall_limit_ms.saturating_sub(remaining_ms);
    let breach = crate::budget::evaluate(
        &request.budgets,
        &task.projection.usage,
        elapsed_ms,
        task.projection.attempt.saturating_sub(1),
    );
    if !task.projection.status.is_terminal()
        && let Some(breach) = breach
    {
        let _interrupted = client
            .request(
                "turn/interrupt",
                json!({"threadId": thread_id, "turnId": turn_id}),
            )
            .await;
        task.append(
            "budget.exceeded",
            json!({"boundary": breach}),
            None,
            None,
            now()?,
        )
        .await?;
    }
    Ok(())
}

pub(crate) async fn finish_terminal(
    mut task: TaskJournal<'_>,
    handle: TaskHandle,
    workspace: Workspace,
    request: &TaskRequest,
    evidence: WorkspaceEvidence,
    mut terminal: NormalizedEvent,
    started: Instant,
) -> Result<RunResult> {
    let elapsed = elapsed_ms(started)?;
    if let Some(data) = terminal.data.as_object_mut() {
        data.insert("wall_ms".to_owned(), Value::from(elapsed));
    }
    let Some(database) = task.database else {
        task.append_normalized(terminal).await?;
        return finish(task, handle, workspace, request, evidence, started).await;
    };
    let projection = task.preview_normalized(&terminal)?;
    let mut receipt = build_receipt(&projection, request, evidence, elapsed)?;
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
    started: Instant,
) -> Result<RunResult> {
    let elapsed = elapsed_ms(started)?;
    let mut receipt = build_receipt(&task.projection, request, evidence, elapsed)?;
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

async fn close_with_error(client: CodexClient, error: Error) -> Result<RunResult> {
    let close_result = client.close().await;
    combine_close(error, close_result)
}

fn combine_close<T>(error: Error, close_result: Result<()>) -> Result<T> {
    match close_result {
        Ok(()) => Err(error),
        Err(close_error) => Err(Error::new(
            error.kind(),
            format!("{error}; App Server shutdown also failed: {close_error}"),
        )),
    }
}

async fn ensure_model(client: &mut CodexClient, requested: &str) -> Result<()> {
    let response = client
        .request("model/list", json!({"limit": 100, "includeHidden": true}))
        .await?;
    let models = response
        .get("data")
        .and_then(Value::as_array)
        .ok_or_else(|| Error::new(ErrorKind::EngineProtocol, "model/list returned no data"))?;
    let available = models.iter().any(|model| {
        model.get("model").and_then(Value::as_str) == Some(requested)
            || model.get("id").and_then(Value::as_str) == Some(requested)
    });
    if !available {
        return Err(Error::new(
            ErrorKind::InvalidInput,
            format!("requested model {requested} is unavailable"),
        ));
    }
    Ok(())
}

async fn append_diff(task: &mut TaskJournal<'_>, evidence: &WorkspaceEvidence) -> Result<()> {
    let count = u64::try_from(evidence.changed_paths.len())
        .map_err(|error| Error::new(ErrorKind::InvalidInput, error.to_string()))?;
    task.append(
        "workspace.diff.updated",
        json!({"diff_hash": evidence.diff_hash, "changed_files": count}),
        None,
        None,
        now()?,
    )
    .await?;
    Ok(())
}
