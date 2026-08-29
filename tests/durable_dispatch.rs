//! Crash-closure tests for durable queue intent and uncertain execution.

use spewer::protocol::{TaskRequest, TaskStatus};
use spewer::store::{Database, EventInput};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

#[tokio::test(flavor = "current_thread")]
async fn pristine_lease_returns_to_the_queue_after_restart()
-> Result<(), Box<dyn std::error::Error>> {
    let path = temporary("requeue")?.with_extension("sqlite");
    let database = Database::open(path.clone()).await?;
    let request = request("requeue-key")?;
    database
        .accept(
            request,
            "requeue-task".to_owned(),
            "2026-08-29T00:00:00Z".to_owned(),
        )
        .await?;
    database
        .lease(
            lease_event("requeue-task"),
            "lease-one".to_owned(),
            "server-old".to_owned(),
            "worker-old".to_owned(),
            "2026-08-29T00:01:00Z".to_owned(),
        )
        .await?;
    database.close().await?;

    let reopened = Database::open(path.clone()).await?;
    let recovery = reopened.recover_dispatches().await?;
    assert_eq!(recovery.runnable.len(), 1);
    assert!(recovery.uncertain.is_empty());
    assert_eq!(
        reopened.dispatch_state("requeue-task".to_owned()).await?,
        Some("queued".to_owned())
    );
    reopened.close().await?;
    remove_database_files(&path)?;
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn observable_worker_start_escalates_instead_of_replaying()
-> Result<(), Box<dyn std::error::Error>> {
    let path = temporary("uncertain")?.with_extension("sqlite");
    let database = Database::open(path.clone()).await?;
    let request = request("uncertain-key")?;
    database
        .accept(
            request,
            "uncertain-task".to_owned(),
            "2026-08-29T00:00:00Z".to_owned(),
        )
        .await?;
    database
        .lease(
            lease_event("uncertain-task"),
            "lease-two".to_owned(),
            "server-old".to_owned(),
            "worker-old".to_owned(),
            "2026-08-29T00:01:00Z".to_owned(),
        )
        .await?;
    database
        .append(EventInput {
            task_id: "uncertain-task".to_owned(),
            attempt: 1,
            kind: "workspace.prepared".to_owned(),
            data: serde_json::json!({"path":"/tmp/worktree","base_revision":"abc"}),
            source: None,
            source_key: None,
            observed_at: "2026-08-29T00:00:02Z".to_owned(),
        })
        .await?;
    database.close().await?;

    let reopened = Database::open(path.clone()).await?;
    let recovery = reopened.recover_dispatches().await?;
    assert!(recovery.runnable.is_empty());
    assert_eq!(recovery.uncertain.len(), 1);
    reopened
        .reconcile_uncertain(
            "uncertain-task".to_owned(),
            "test crash boundary".to_owned(),
        )
        .await?;
    let result = reopened.result("uncertain-task".to_owned()).await?;
    assert_eq!(result.projection.status, TaskStatus::Escalated);
    assert!(result.message.is_some());
    assert_eq!(
        reopened.dispatch_state("uncertain-task".to_owned()).await?,
        Some("terminal".to_owned())
    );
    reopened.close().await?;
    remove_database_files(&path)?;
    Ok(())
}

fn request(key: &str) -> Result<TaskRequest, Box<dyn std::error::Error>> {
    let mut request: TaskRequest =
        serde_json::from_str(include_str!("fixtures/task-request.json"))?;
    key.clone_into(&mut request.idempotency_key);
    request.task_id = None;
    Ok(request)
}

fn lease_event(task_id: &str) -> EventInput {
    EventInput {
        task_id: task_id.to_owned(),
        attempt: 1,
        kind: "turn.leased".to_owned(),
        data: serde_json::json!({"lease_id":"lease","worker_id":"worker"}),
        source: None,
        source_key: None,
        observed_at: "2026-08-29T00:00:01Z".to_owned(),
    }
}

fn temporary(name: &str) -> Result<PathBuf, Box<dyn std::error::Error>> {
    let nanos = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
    Ok(std::env::temp_dir().join(format!(
        "spewer-dispatch-{name}-{}-{nanos}",
        std::process::id()
    )))
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
