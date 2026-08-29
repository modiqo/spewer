//! Checkpoint creation and conservative Codex reconciliation.

use crate::codex::{CodexClient, CodexConfig};
use crate::error::{Error, ErrorKind, Result};
use crate::protocol::{Checkpoint, PROTOCOL_VERSION};
use crate::reducer::{PROJECTION_VERSION, Projection};
use crate::store::Database;
use crate::util::{new_id, now, sha256};
use crate::workspace::WorkspaceEvidence;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use tokio::process::Command;

/// Result of validating and restoring one Codex thread.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct RecoveryReport {
    /// Task selected for reconciliation.
    pub task_id: String,
    /// Checkpoint used as the recovery boundary.
    pub checkpoint_id: String,
    /// Native thread returned by `thread/read`.
    pub thread: Value,
    /// Native thread returned by `thread/resume`.
    pub resumed: Value,
}

/// Reconciles and continues one retained nonterminal Codex task.
pub async fn resume_codex(
    database: &Database,
    task_id: String,
    config: CodexConfig,
) -> Result<crate::runner::RunResult> {
    crate::resume::run(task_id, config, database).await
}

/// Creates a checkpoint from three independent state records.
pub fn checkpoint(
    projection: &Projection,
    evidence: &WorkspaceEvidence,
    reason: &str,
    resumable: bool,
) -> Result<Checkpoint> {
    Ok(Checkpoint {
        protocol_version: PROTOCOL_VERSION.to_owned(),
        checkpoint_id: new_id("cp")?,
        task_id: projection.task_id.clone(),
        attempt: projection.attempt,
        event_seq: projection.event_seq,
        projection_version: PROJECTION_VERSION,
        engine: serde_json::to_value(&projection.engine)?,
        workspace: json!({
            "path": projection.workspace.path,
            "base_revision": projection.workspace.base_revision,
            "diff_hash": evidence.diff_hash,
            "artifact": evidence.artifact,
        }),
        resumable,
        reason: reason.to_owned(),
        created_at: now()?,
    })
}

/// Loads and validates the latest resumable checkpoint.
pub async fn load_validated(database: &Database, task_id: String) -> Result<Checkpoint> {
    let checkpoint = database
        .latest_checkpoint(task_id)
        .await?
        .ok_or_else(|| Error::new(ErrorKind::InvalidInput, "task has no checkpoint"))?;
    if !checkpoint.resumable {
        return Err(Error::new(
            ErrorKind::InvalidInput,
            "latest checkpoint is not resumable",
        ));
    }
    validate_workspace(&checkpoint).await?;
    Ok(checkpoint)
}

/// Reopens the stored Codex thread after workspace validation.
pub async fn reconcile_codex(
    database: &Database,
    task_id: String,
    config: CodexConfig,
) -> Result<RecoveryReport> {
    let checkpoint = load_validated(database, task_id.clone()).await?;
    let thread_id = required(&checkpoint.engine, "thread_id")?;
    let model = required(&checkpoint.engine, "requested_model")?;
    let cwd = required(&checkpoint.workspace, "path")?;
    let mut client = CodexClient::connect(config).await?;
    let thread_result = client
        .request(
            "thread/read",
            json!({"threadId": thread_id, "includeTurns": true}),
        )
        .await;
    let thread = match thread_result {
        Ok(value) => value,
        Err(error) => {
            client.close().await?;
            return Err(error);
        }
    };
    let resumed_result = client
        .request(
            "thread/resume",
            json!({
                "threadId": thread_id,
                "cwd": cwd,
                "model": model,
                "approvalPolicy": "never",
                "sandbox": "workspace-write"
            }),
        )
        .await;
    let resumed = match resumed_result {
        Ok(value) => value,
        Err(error) => {
            client.close().await?;
            return Err(error);
        }
    };
    client.close().await?;
    Ok(RecoveryReport {
        task_id,
        checkpoint_id: checkpoint.checkpoint_id,
        thread,
        resumed,
    })
}

async fn validate_workspace(checkpoint: &Checkpoint) -> Result<()> {
    let path = PathBuf::from(required(&checkpoint.workspace, "path")?);
    let expected_base = required(&checkpoint.workspace, "base_revision")?;
    let expected_diff = required(&checkpoint.workspace, "diff_hash")?;
    let actual_base = git(&path, &["rev-parse", "HEAD"]).await?;
    if actual_base.trim() != expected_base {
        return Err(Error::new(
            ErrorKind::InvalidInput,
            "checkpoint workspace base revision changed",
        ));
    }
    let diff = git_bytes(&path, &["diff", "--binary", "--no-ext-diff", "HEAD"]).await?;
    if sha256(&diff)? != expected_diff {
        return Err(Error::new(
            ErrorKind::InvalidInput,
            "checkpoint workspace diff changed",
        ));
    }
    Ok(())
}

async fn git(path: &Path, arguments: &[&str]) -> Result<String> {
    String::from_utf8(git_bytes(path, arguments).await?)
        .map_err(|error| Error::new(ErrorKind::Io, error.to_string()))
}

async fn git_bytes(path: &Path, arguments: &[&str]) -> Result<Vec<u8>> {
    let output = Command::new("git")
        .arg("-C")
        .arg(path)
        .args(arguments)
        .output()
        .await?;
    if !output.status.success() {
        return Err(Error::new(
            ErrorKind::Io,
            String::from_utf8_lossy(&output.stderr).into_owned(),
        ));
    }
    Ok(output.stdout)
}

fn required(value: &Value, field: &str) -> Result<String> {
    value
        .get(field)
        .and_then(Value::as_str)
        .map(str::to_owned)
        .ok_or_else(|| Error::new(ErrorKind::InvalidInput, format!("checkpoint lacks {field}")))
}
