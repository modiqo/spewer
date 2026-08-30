use crate::codex::{CodexClient, CodexConfig, turn_params};
use crate::error::{Error, ErrorKind, Result};
use crate::journal::TaskJournal;
use crate::protocol::{PROTOCOL_VERSION, TaskHandle};
use crate::runner::{DriveOptions, RunResult, drive, finish, finish_terminal};
use crate::store::Database;
use crate::util::now;
use crate::workspace::Workspace;
use serde_json::{Value, json};
use std::time::Duration;
use tokio::time::Instant;

pub(crate) async fn run(
    task_id: String,
    config: CodexConfig,
    database: &Database,
) -> Result<RunResult> {
    let request = database.request(task_id.clone()).await?;
    request.validate()?;
    let _checkpoint = crate::recovery::load_validated(database, task_id.clone()).await?;
    let projection = database
        .get(task_id.clone())
        .await?
        .ok_or_else(|| Error::new(ErrorKind::InvalidInput, "task does not exist"))?;
    if projection.status.is_terminal() {
        return Err(Error::new(
            ErrorKind::InvalidInput,
            "terminal task cannot resume",
        ));
    }
    let workspace = Workspace::restore(&request, &projection).await?;
    let thread_id = projection
        .engine
        .thread_id
        .clone()
        .ok_or_else(|| Error::new(ErrorKind::InvalidInput, "task has no Codex thread"))?;
    let handle = TaskHandle {
        protocol_version: PROTOCOL_VERSION.to_owned(),
        task_id,
        status: projection.status,
        event_cursor: projection.event_seq,
        created_at: projection.created_at.clone(),
    };
    let mut task = TaskJournal {
        projection,
        events: Vec::new(),
        database: Some(database),
    };
    let started = Instant::now();
    let deadline = started
        .checked_add(Duration::from_secs(request.budgets.wall_seconds))
        .ok_or_else(|| Error::new(ErrorKind::InvalidInput, "wall deadline overflow"))?;
    let mut client = restore_client(config, &request, &workspace, &thread_id).await?;
    task.append("task.resumed", json!({}), None, None, now()?)
        .await?;
    let turn = client
        .request("turn/start", turn_params(&request, &workspace, &thread_id)?)
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
    let driven = drive(
        &mut client,
        &request,
        &workspace,
        &mut task,
        &thread_id,
        &turn_id,
        DriveOptions {
            deadline,
            input: None,
        },
    )
    .await;
    let close = client.close().await;
    let driven = driven?;
    close?;
    match driven.terminal {
        Some(terminal) => {
            finish_terminal(
                task,
                handle,
                workspace,
                &request,
                driven.evidence,
                terminal,
                crate::runner::EngineRunMeta {
                    started,
                    version: Some("codex-cli".to_owned()),
                },
            )
            .await
        }
        None => {
            finish(
                task,
                handle,
                workspace,
                &request,
                driven.evidence,
                crate::runner::EngineRunMeta {
                    started,
                    version: Some("codex-cli".to_owned()),
                },
            )
            .await
        }
    }
}

async fn restore_client(
    mut config: CodexConfig,
    request: &crate::protocol::TaskRequest,
    workspace: &Workspace,
    thread_id: &str,
) -> Result<CodexClient> {
    for name in &request.permissions.environment_allowlist {
        if !config.inherited_environment.contains(name) {
            config.inherited_environment.push(name.clone());
        }
    }
    let mut client = CodexClient::connect(config).await?;
    client
        .request(
            "thread/read",
            json!({"threadId": thread_id, "includeTurns": true}),
        )
        .await?;
    let sandbox = crate::codex::sandbox_name(request);
    client
        .request(
            "thread/resume",
            json!({
                "threadId": thread_id,
                "cwd": workspace.path,
                "model": request.engine.model,
                "approvalPolicy": "never",
                "sandbox": sandbox
            }),
        )
        .await?;
    Ok(client)
}

fn required_pointer(value: &Value, pointer: &str) -> Result<String> {
    value
        .pointer(pointer)
        .and_then(Value::as_str)
        .map(str::to_owned)
        .ok_or_else(|| Error::new(ErrorKind::EngineProtocol, format!("missing {pointer}")))
}
