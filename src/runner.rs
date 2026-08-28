//! One bounded engine run from validated request to receipt.

use crate::codex::{CodexClient, CodexConfig, CodexMessage, NormalizedEvent, Normalizer};
use crate::error::{Error, ErrorKind, Result};
use crate::protocol::{
    Event, PROTOCOL_VERSION, Receipt, ReceiptEngine, ReceiptStatus, TaskHandle, TaskRequest, Usage,
    Verification,
};
use crate::reducer::{Projection, apply};
use crate::util::{new_id, now};
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
    /// Isolated worktree retained for parent inspection.
    pub workspace: Workspace,
}

struct MemoryTask {
    projection: Projection,
    events: Vec<Event>,
}

impl MemoryTask {
    fn append(
        &mut self,
        kind: &str,
        data: Value,
        source: Option<crate::protocol::EventSource>,
        observed_at: String,
    ) -> Result<Event> {
        let seq = self
            .projection
            .event_seq
            .checked_add(1)
            .ok_or_else(|| Error::new(ErrorKind::InvalidInput, "event sequence exhausted"))?;
        let event = Event {
            protocol_version: PROTOCOL_VERSION.to_owned(),
            task_id: self.projection.task_id.clone(),
            attempt: self.projection.attempt,
            seq,
            kind: kind.to_owned(),
            observed_at,
            data,
            source,
        };
        self.projection = apply(&self.projection, &event)?;
        self.events.push(event.clone());
        Ok(event)
    }

    fn append_normalized(&mut self, event: NormalizedEvent) -> Result<Event> {
        self.append(
            &event.kind,
            event.data,
            Some(event.source),
            event.observed_at,
        )
    }
}

/// Runs one task through Codex App Server and returns a typed receipt.
pub async fn run_codex(request: TaskRequest, config: CodexConfig) -> Result<RunResult> {
    request.validate()?;
    if request.engine.kind != "codex-app-server" {
        return Err(Error::new(
            ErrorKind::InvalidInput,
            "run_codex requires codex-app-server",
        ));
    }
    let (task_id, handle, mut task) = accept_task(&request)?;
    let workspace = Workspace::prepare(&request, &task_id).await?;
    record_workspace(&mut task, &workspace)?;
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
    let evidence = match outcome {
        Ok(evidence) => evidence,
        Err(error) => return combine_close(error, close_result),
    };
    close_result?;
    finish(task, handle, workspace, &request, evidence, started)
}

fn accept_task(request: &TaskRequest) -> Result<(String, TaskHandle, MemoryTask)> {
    let task_id = match &request.task_id {
        Some(task_id) => task_id.clone(),
        None => new_id("tsk")?,
    };
    let created_at = now()?;
    let mut task = MemoryTask {
        projection: Projection::initial(task_id.clone(), request, created_at.clone()),
        events: Vec::new(),
    };
    task.append(
        "task.accepted",
        json!({"idempotency_key": request.idempotency_key}),
        None,
        created_at.clone(),
    )?;
    let handle = TaskHandle {
        protocol_version: PROTOCOL_VERSION.to_owned(),
        task_id: task_id.clone(),
        status: task.projection.status,
        event_cursor: task.projection.event_seq,
        created_at,
    };
    Ok((task_id, handle, task))
}

fn record_workspace(task: &mut MemoryTask, workspace: &Workspace) -> Result<()> {
    task.append(
        "workspace.prepared",
        json!({"path": workspace.path, "base_revision": workspace.base_revision}),
        None,
        now()?,
    )?;
    task.append("engine.starting", json!({}), None, now()?)?;
    Ok(())
}

async fn start_conversation(
    client: &mut CodexClient,
    request: &TaskRequest,
    workspace: &Workspace,
    task: &mut MemoryTask,
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
        now()?,
    )?;
    let turn = client
        .request("turn/start", turn_params(request, workspace, &thread_id)?)
        .await?;
    let turn_id = required_pointer(&turn, "/turn/id")?;
    task.append("turn.started", json!({"turn_id": turn_id}), None, now()?)?;
    Ok((thread_id, turn_id))
}

async fn drive(
    client: &mut CodexClient,
    request: &TaskRequest,
    workspace: &Workspace,
    task: &mut MemoryTask,
    thread_id: &str,
    turn_id: &str,
    deadline: Instant,
) -> Result<WorkspaceEvidence> {
    let mut normalizer = Normalizer::default();
    let mut evidence = None;
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
                    now()?,
                )?;
                break;
            }
        };
        let mapped = normalizer.normalize(message)?;
        if mapped.kind == "turn.completed" {
            let captured = workspace
                .capture(&request.permissions.writable_paths)
                .await?;
            append_diff(task, &captured)?;
            evidence = Some(captured);
        }
        let input_required = mapped.kind == "input.required";
        task.append_normalized(mapped)?;
        if input_required {
            task.append(
                "task.escalated",
                json!({"reason":"parent input required"}),
                None,
                now()?,
            )?;
        }
    }
    Ok(match evidence {
        Some(evidence) => evidence,
        None => {
            workspace
                .capture(&request.permissions.writable_paths)
                .await?
        }
    })
}

fn finish(
    mut task: MemoryTask,
    handle: TaskHandle,
    workspace: Workspace,
    request: &TaskRequest,
    evidence: WorkspaceEvidence,
    started: Instant,
) -> Result<RunResult> {
    let elapsed = u64::try_from(started.elapsed().as_millis())
        .map_err(|error| Error::new(ErrorKind::InvalidInput, error.to_string()))?;
    task.projection.usage.wall_ms = elapsed;
    let receipt = build_receipt(&task.projection, request, evidence, elapsed)?;
    Ok(RunResult {
        handle,
        events: task.events,
        projection: task.projection,
        receipt,
        workspace,
    })
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

fn thread_params(request: &TaskRequest, workspace: &Workspace) -> Value {
    json!({
        "model": request.engine.model,
        "cwd": workspace.path,
        "approvalPolicy": "never",
        "sandbox": sandbox_name(request),
        "serviceName": "spewer"
    })
}

fn turn_params(request: &TaskRequest, workspace: &Workspace, thread_id: &str) -> Result<Value> {
    let mut parameters = json!({
        "threadId": thread_id,
        "input": [{"type":"text", "text": task_prompt(request)}],
        "cwd": workspace.path,
        "approvalPolicy": "never",
        "sandboxPolicy": sandbox_policy(request),
        "model": request.engine.model
    });
    if let Some(effort) = &request.engine.effort {
        let object = parameters.as_object_mut().ok_or_else(|| {
            Error::new(
                ErrorKind::EngineProtocol,
                "turn parameters are not an object",
            )
        })?;
        object.insert("effort".to_owned(), Value::String(effort.clone()));
    }
    Ok(parameters)
}

fn sandbox_name(request: &TaskRequest) -> &str {
    if request.permissions.filesystem == "read-only" {
        "read-only"
    } else {
        "workspace-write"
    }
}

fn sandbox_policy(request: &TaskRequest) -> Value {
    if request.permissions.filesystem == "read-only" {
        return json!({"type":"readOnly"});
    }
    json!({
        "type":"workspaceWrite",
        "writableRoots": [],
        "networkAccess": request.permissions.network == "allow"
    })
}

fn task_prompt(request: &TaskRequest) -> String {
    let acceptance = request
        .acceptance
        .iter()
        .map(|item| format!("- {item}"))
        .collect::<Vec<_>>()
        .join("\n");
    let files = request.context.files.join(", ");
    let notes = request.context.notes.join("\n");
    format!(
        "Objective:\n{}\n\nAcceptance criteria:\n{}\n\nProjected files:\n{}\n\nConstraints:\n{}\n\nWork only inside the supplied repository. Run focused verification when possible. Finish with a concise summary and the verification you ran.",
        request.objective, acceptance, files, notes
    )
}

fn append_diff(task: &mut MemoryTask, evidence: &WorkspaceEvidence) -> Result<()> {
    let count = u64::try_from(evidence.changed_paths.len())
        .map_err(|error| Error::new(ErrorKind::InvalidInput, error.to_string()))?;
    task.append(
        "workspace.diff.updated",
        json!({"diff_hash": evidence.diff_hash, "changed_files": count}),
        None,
        now()?,
    )?;
    Ok(())
}

fn build_receipt(
    projection: &Projection,
    request: &TaskRequest,
    evidence: WorkspaceEvidence,
    wall_ms: u64,
) -> Result<Receipt> {
    let status = match projection.status {
        crate::protocol::TaskStatus::Completed => ReceiptStatus::Completed,
        crate::protocol::TaskStatus::Cancelled => ReceiptStatus::Cancelled,
        crate::protocol::TaskStatus::Escalated | crate::protocol::TaskStatus::InputRequired => {
            ReceiptStatus::Escalated
        }
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

fn required_pointer(value: &Value, pointer: &str) -> Result<String> {
    value
        .pointer(pointer)
        .and_then(Value::as_str)
        .map(str::to_owned)
        .ok_or_else(|| Error::new(ErrorKind::EngineProtocol, format!("missing {pointer}")))
}
