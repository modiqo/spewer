//! Recovery checkpoint and durable callback integration tests.

use spewer::protocol::{
    Artifact, EventSource, Receipt, ReceiptEngine, ReceiptStatus, TaskRequest, Usage,
};
use spewer::recovery::{checkpoint, load_validated};
use spewer::store::{Database, EventInput};
use spewer::workspace::WorkspaceEvidence;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

#[tokio::test(flavor = "current_thread")]
async fn changed_workspace_blocks_recovery() -> Result<(), Box<dyn std::error::Error>> {
    let root = temporary("workspace")?;
    std::fs::create_dir_all(&root)?;
    git(&root, &["init", "-q"])?;
    git(&root, &["config", "user.email", "spewer@example.invalid"])?;
    git(&root, &["config", "user.name", "Spewer Test"])?;
    std::fs::write(root.join("base.txt"), "base\n")?;
    git(&root, &["add", "base.txt"])?;
    git(&root, &["commit", "-qm", "base"])?;
    let base = git_output(&root, &["rev-parse", "HEAD"])?;
    let path = temporary("checkpoint")?.with_extension("sqlite");
    let database = Database::open(path.clone()).await?;
    let mut request: TaskRequest =
        serde_json::from_str(include_str!("fixtures/task-request.json"))?;
    request.workspace.path = root.to_string_lossy().into_owned();
    request.idempotency_key = "recovery-boundary".to_owned();
    let accepted = database
        .accept(
            request,
            "recovery-task".to_owned(),
            "2026-08-28T00:00:00Z".to_owned(),
        )
        .await?;
    let mut projection = accepted.projection;
    projection.workspace.path = root.to_string_lossy().into_owned();
    projection.workspace.base_revision = base.trim().to_owned();
    projection.engine.thread_id = Some("thread-one".to_owned());
    let empty_hash = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855".to_owned();
    let evidence = WorkspaceEvidence {
        diff_hash: empty_hash.clone(),
        changed_paths: Vec::new(),
        artifact: Artifact {
            kind: "git-diff".to_owned(),
            uri: format!("artifact://sha256/{empty_hash}"),
            sha256: empty_hash,
        },
    };
    database
        .save_checkpoint(checkpoint(&projection, &evidence, "turn completed", true)?)
        .await?;
    assert!(
        load_validated(&database, "recovery-task".to_owned())
            .await
            .is_ok()
    );
    std::fs::write(root.join("changed.txt"), "changed\n")?;
    git(&root, &["add", "--intent-to-add", "changed.txt"])?;
    assert!(
        load_validated(&database, "recovery-task".to_owned())
            .await
            .is_err()
    );
    database.close().await?;
    remove_database_files(&path)?;
    std::fs::remove_dir_all(root)?;
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn outbox_retries_stable_message_until_acknowledged() -> Result<(), Box<dyn std::error::Error>>
{
    let path = temporary("outbox")?.with_extension("sqlite");
    let database = Database::open(path.clone()).await?;
    let request: TaskRequest = serde_json::from_str(include_str!("fixtures/task-request.json"))?;
    database
        .accept(
            request,
            "delivery-task".to_owned(),
            "2026-08-28T00:00:00Z".to_owned(),
        )
        .await?;
    let mut receipt = fixture_receipt();
    receipt.final_event_seq = 2;
    let terminal = terminal_input();
    let first = database
        .finalize(terminal.clone(), receipt.clone(), "poll".to_owned())
        .await?;
    let duplicate = database
        .finalize(terminal, receipt, "poll".to_owned())
        .await?;
    let pending = database.pending("play".to_owned()).await?;
    let unauthorized_pending = database.pending("another-harness".to_owned()).await?;
    let applied = database
        .acknowledge(first.message.message_id.clone(), "play".to_owned())
        .await?;
    let unauthorized = database
        .acknowledge(
            first.message.message_id.clone(),
            "another-harness".to_owned(),
        )
        .await;
    let repeated = database
        .acknowledge(first.message.message_id.clone(), "play".to_owned())
        .await?;
    let remaining = database.pending("play".to_owned()).await?;
    let result_after_ack = database.result("delivery-task".to_owned()).await?;
    let observed = database.observe("delivery-task".to_owned(), 1).await?;
    database.close().await?;
    assert!(first.append.inserted);
    assert!(!duplicate.append.inserted);
    assert_eq!(first.message, duplicate.message);
    assert_eq!(pending, vec![first.message.clone()]);
    assert!(unauthorized_pending.is_empty());
    assert!(applied);
    assert!(unauthorized.is_err());
    assert!(!repeated);
    assert!(remaining.is_empty());
    assert_eq!(result_after_ack.message, Some(first.message));
    assert_eq!(observed.next_cursor, 2);
    assert_eq!(observed.events.len(), 1);
    assert_eq!(observed.events.first().map(|event| event.seq), Some(2));
    remove_database_files(&path)?;
    Ok(())
}

fn terminal_input() -> EventInput {
    EventInput {
        task_id: "delivery-task".to_owned(),
        attempt: 1,
        kind: "turn.completed".to_owned(),
        data: serde_json::json!({"status":"completed"}),
        source: Some(EventSource {
            engine: "fake".to_owned(),
            method: "turn/completed".to_owned(),
            thread_id: Some("fake-thread".to_owned()),
            turn_id: Some("fake-turn".to_owned()),
            item_id: None,
            payload_hash: "terminal-hash".to_owned(),
        }),
        source_key: Some("fake:turn/completed:1".to_owned()),
        observed_at: "2026-08-28T00:00:01Z".to_owned(),
    }
}

fn fixture_receipt() -> Receipt {
    Receipt {
        protocol_version: "0.1".to_owned(),
        receipt_id: "receipt-one".to_owned(),
        task_id: "delivery-task".to_owned(),
        attempt: 1,
        status: ReceiptStatus::Completed,
        summary: "done".to_owned(),
        artifacts: Vec::new(),
        verification: Vec::new(),
        verification_waiver: Some("fixture".to_owned()),
        usage: Usage::default(),
        engine: ReceiptEngine {
            kind: "fake".to_owned(),
            requested_model: "fake".to_owned(),
            observed_models: vec!["fake".to_owned()],
            version: Some("1".to_owned()),
        },
        capsule: None,
        final_event_seq: 1,
        completed_at: "2026-08-28T00:00:01Z".to_owned(),
    }
}

fn git(directory: &Path, arguments: &[&str]) -> Result<(), Box<dyn std::error::Error>> {
    let output = Command::new("git")
        .arg("-C")
        .arg(directory)
        .args(arguments)
        .output()?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).into_owned().into());
    }
    Ok(())
}

fn git_output(directory: &Path, arguments: &[&str]) -> Result<String, Box<dyn std::error::Error>> {
    let output = Command::new("git")
        .arg("-C")
        .arg(directory)
        .args(arguments)
        .output()?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).into_owned().into());
    }
    Ok(String::from_utf8(output.stdout)?)
}

fn temporary(name: &str) -> Result<PathBuf, Box<dyn std::error::Error>> {
    let nanos = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
    Ok(std::env::temp_dir().join(format!("spewer-{name}-{}-{nanos}", std::process::id())))
}

fn remove_database_files(path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    for candidate in [
        path.to_path_buf(),
        PathBuf::from(format!("{}-shm", path.display())),
        PathBuf::from(format!("{}-wal", path.display())),
    ] {
        match std::fs::remove_file(candidate) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
    }
    Ok(())
}
