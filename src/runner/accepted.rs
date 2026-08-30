//! Entry and failure paths for tasks accepted by the supervisor.

use super::{RunResult, run_codex_inner};
use crate::codex::{CodexClient, CodexConfig};
use crate::error::{Error, ErrorKind, Result};
use crate::journal::TaskJournal;
use crate::protocol::{Event, PROTOCOL_VERSION, TaskHandle, TaskRequest};
use crate::receipt::build_failure_receipt;
use crate::store::{Database, EventInput};
use crate::util::now;
use serde_json::json;
use tokio::sync::mpsc;

/// Runs a task that a supervisor already accepted and leased.
pub async fn run_codex_accepted(
    request: TaskRequest,
    task_id: String,
    lease_id: String,
    config: CodexConfig,
    database: &Database,
    input: mpsc::Receiver<crate::protocol::TaskInputResponse>,
) -> Result<RunResult> {
    run_codex_inner(
        request,
        config,
        Some(database),
        Some(task_id),
        Some(lease_id),
        Some(input),
    )
    .await
}

/// Runs an Ollama task that a supervisor already accepted and leased.
pub async fn run_ollama_accepted(
    request: TaskRequest,
    task_id: String,
    lease_id: String,
    config: crate::ollama::OllamaConfig,
    database: &Database,
) -> Result<RunResult> {
    super::adapter::run_ollama_inner(
        request,
        config,
        Some(database),
        Some(task_id),
        Some(lease_id),
    )
    .await
}

pub(super) async fn connect_engine(
    config: CodexConfig,
    database: Option<&Database>,
    task_id: &str,
    lease_id: Option<&str>,
) -> Result<CodexClient> {
    let Some(lease_id) = lease_id else {
        return CodexClient::connect(config).await;
    };
    let database = database.ok_or_else(|| {
        Error::new(
            ErrorKind::InvalidInput,
            "leased engine startup requires durable storage",
        )
    })?;
    let mut client = CodexClient::spawn_uninitialized(&config)?;
    let process_group = client.process_group().ok_or_else(|| {
        Error::new(
            ErrorKind::Io,
            "App Server did not expose a process group identity",
        )
    })?;
    database
        .register_process(
            task_id.to_owned(),
            lease_id.to_owned(),
            process_group,
            config.program.to_string_lossy().into_owned(),
            now()?,
        )
        .await?;
    client.initialize(config.startup_timeout).await?;
    Ok(client)
}

pub(super) async fn load_accepted_task<'a>(
    request: &TaskRequest,
    database: Option<&'a Database>,
    task_id: String,
) -> Result<(String, TaskHandle, TaskJournal<'a>)> {
    let database = database.ok_or_else(|| {
        Error::new(
            ErrorKind::InvalidInput,
            "accepted tasks require durable storage",
        )
    })?;
    let stored = database.request(task_id.clone()).await?;
    if crate::util::request_hash(&stored)? != crate::util::request_hash(request)? {
        return Err(Error::new(
            ErrorKind::InvalidInput,
            "accepted task request hash does not match durable state",
        ));
    }
    let projection = database
        .get(task_id.clone())
        .await?
        .ok_or_else(|| Error::new(ErrorKind::InvalidInput, "accepted task does not exist"))?;
    if projection.status.is_terminal() {
        return Err(Error::new(
            ErrorKind::InvalidInput,
            "accepted task is already terminal",
        ));
    }
    let handle = TaskHandle {
        protocol_version: PROTOCOL_VERSION.to_owned(),
        task_id: task_id.clone(),
        status: projection.status,
        event_cursor: projection.event_seq,
        created_at: projection.created_at.clone(),
    };
    let journal = TaskJournal {
        projection,
        events: Vec::new(),
        database: Some(database),
    };
    Ok((task_id, handle, journal))
}

/// Converts a scheduled worker failure into one durable terminal callback.
pub async fn fail_durable(
    database: &Database,
    request: &TaskRequest,
    task_id: String,
    error: &Error,
) -> Result<()> {
    let Some(current) = database.get(task_id.clone()).await? else {
        return Err(Error::new(
            ErrorKind::InvalidInput,
            "failed task does not exist",
        ));
    };
    if current.status.is_terminal() {
        return Ok(());
    }
    let observed_at = now()?;
    let reason = format!("worker failed with {:?}", error.kind());
    let event = Event {
        protocol_version: PROTOCOL_VERSION.to_owned(),
        task_id: task_id.clone(),
        attempt: current.attempt,
        seq: current
            .event_seq
            .checked_add(1)
            .ok_or_else(|| Error::new(ErrorKind::InvalidInput, "event sequence exhausted"))?,
        kind: "task.failed".to_owned(),
        observed_at: observed_at.clone(),
        data: json!({"reason": reason}),
        source: None,
    };
    let projection = crate::reducer::apply(&current, &event)?;
    let receipt = build_failure_receipt(&projection, request)?;
    let _finalized = database
        .finalize(
            EventInput {
                task_id,
                attempt: current.attempt,
                kind: event.kind,
                data: event.data,
                source: None,
                source_key: None,
                observed_at,
            },
            receipt,
            request.callback.mode.clone(),
        )
        .await?;
    Ok(())
}
