//! Shared durable lifecycle for finite provider-neutral adapters.

use super::{EngineRunMeta, RunResult, finish, finish_terminal, record_workspace};
use crate::engine::EngineAdapter;
use crate::error::{Error, ErrorKind, Result};
use crate::journal::TaskJournal;
use crate::ollama::{ENGINE_KIND, OllamaConfig, OllamaEngine};
use crate::protocol::{TaskHandle, TaskRequest};
use crate::store::Database;
use crate::util::now;
use crate::workspace::{Workspace, WorkspaceEvidence};
use serde_json::json;
use std::time::Duration;
use tokio::time::Instant;

/// Runs one task through a local Ollama server.
pub async fn run_ollama(request: TaskRequest, config: OllamaConfig) -> Result<RunResult> {
    run_ollama_inner(request, config, None, None, None).await
}

/// Runs one Ollama task while committing accepted events to `SQLite`.
pub async fn run_ollama_durable(
    request: TaskRequest,
    config: OllamaConfig,
    database: &Database,
) -> Result<RunResult> {
    run_ollama_inner(request, config, Some(database), None, None).await
}

pub(super) async fn run_ollama_inner(
    mut request: TaskRequest,
    config: OllamaConfig,
    database: Option<&Database>,
    accepted_task_id: Option<String>,
    _accepted_lease_id: Option<String>,
) -> Result<RunResult> {
    super::request::resolve(&mut request, accepted_task_id.is_some())?;
    if request.engine.kind != ENGINE_KIND {
        return Err(Error::new(
            ErrorKind::InvalidInput,
            "run_ollama requires engine.kind=ollama",
        ));
    }
    let (_task_id, handle, mut task) = match accepted_task_id {
        Some(task_id) => super::accepted::load_accepted_task(&request, database, task_id).await?,
        None => super::accept_task(&request, database).await?,
    };
    let task_id = handle.task_id.clone();
    let workspace = Workspace::prepare(&request, &task_id).await?;
    record_workspace(&mut task, &workspace).await?;
    let started = Instant::now();
    let mut engine = match OllamaEngine::connect(config, &request, &workspace.path).await {
        Ok(engine) => engine,
        Err(error) => return fail_run(database, &request, task_id, error).await,
    };
    let version = Some(format!("ollama {}", engine.version()));
    let timeout = Duration::from_secs(request.budgets.wall_seconds);
    let executed = tokio::time::timeout(timeout, engine.execute(&request)).await;
    let events = match executed {
        Ok(Ok(events)) => events,
        Ok(Err(error)) => return fail_run(database, &request, task_id, error).await,
        Err(_) => {
            task.append(
                "task.cancelled",
                json!({"reason":"wall budget exceeded"}),
                None,
                None,
                now()?,
            )
            .await?;
            let evidence = workspace
                .capture(&request.permissions.writable_paths)
                .await?;
            return finish(
                task,
                handle,
                workspace,
                &request,
                evidence,
                EngineRunMeta { started, version },
            )
            .await;
        }
    };
    consume_events(events, task, handle, workspace, &request, started, version).await
}

async fn fail_run(
    database: Option<&Database>,
    request: &TaskRequest,
    task_id: String,
    error: Error,
) -> Result<RunResult> {
    if let Some(database) = database {
        super::fail_durable(database, request, task_id, &error).await?;
    }
    Err(error)
}

async fn consume_events(
    events: Vec<crate::engine::EngineEvent>,
    mut task: TaskJournal<'_>,
    handle: TaskHandle,
    workspace: Workspace,
    request: &TaskRequest,
    started: Instant,
    version: Option<String>,
) -> Result<RunResult> {
    for event in events {
        let normalized = event.normalize(ENGINE_KIND)?;
        if normalized.kind == "turn.completed" {
            let evidence = capture_terminal(&mut task, request, &workspace).await?;
            return finish_terminal(
                task,
                handle,
                workspace,
                request,
                evidence,
                normalized,
                EngineRunMeta { started, version },
            )
            .await;
        }
        task.append_normalized(normalized).await?;
        if let Some(breach) = crate::budget::evaluate(
            &request.budgets,
            &task.projection.usage,
            elapsed_ms(started)?,
            task.projection.attempt.saturating_sub(1),
        ) {
            task.append(
                "budget.exceeded",
                json!({"boundary":breach}),
                None,
                None,
                now()?,
            )
            .await?;
            let evidence = workspace
                .capture(&request.permissions.writable_paths)
                .await?;
            return finish(
                task,
                handle,
                workspace,
                request,
                evidence,
                EngineRunMeta { started, version },
            )
            .await;
        }
    }
    Err(Error::new(
        ErrorKind::EngineProtocol,
        "Ollama event stream ended without a terminal event",
    ))
}

async fn capture_terminal(
    task: &mut TaskJournal<'_>,
    request: &TaskRequest,
    workspace: &Workspace,
) -> Result<WorkspaceEvidence> {
    let evidence = workspace
        .capture(&request.permissions.writable_paths)
        .await?;
    let count = u64::try_from(evidence.changed_paths.len())
        .map_err(|error| Error::new(ErrorKind::InvalidInput, error.to_string()))?;
    task.append(
        "workspace.diff.updated",
        json!({"diff_hash":evidence.diff_hash,"changed_files":count}),
        None,
        None,
        now()?,
    )
    .await?;
    if let Some(database) = task.database {
        let checkpoint =
            crate::recovery::checkpoint(&task.projection, &evidence, "turn completed", true)?;
        database.save_checkpoint(checkpoint).await?;
    }
    Ok(evidence)
}

fn elapsed_ms(started: Instant) -> Result<u64> {
    u64::try_from(started.elapsed().as_millis())
        .map_err(|error| Error::new(ErrorKind::InvalidInput, error.to_string()))
}
