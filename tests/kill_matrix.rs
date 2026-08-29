//! Explicit restart barriers for each durable database boundary.

use spewer::protocol::{
    EventSource, Receipt, ReceiptEngine, ReceiptStatus, TaskRequest, TaskStatus, Usage,
};
use spewer::security::{EffectState, transition};
use spewer::store::{Database, EventInput};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

#[tokio::test(flavor = "current_thread")]
async fn committed_boundaries_survive_and_uncommitted_work_replays()
-> Result<(), Box<dyn std::error::Error>> {
    let path = database_path()?;
    let database = Database::open(path.clone()).await?;
    assert!(database.get("kill-task".to_owned()).await?.is_none());
    let request: TaskRequest = serde_json::from_str(include_str!("fixtures/task-request.json"))?;
    let accepted = database
        .accept(
            request,
            "kill-task".to_owned(),
            "2026-08-28T00:00:00Z".to_owned(),
        )
        .await?;
    assert_eq!(accepted.projection.status, TaskStatus::Queued);
    database.close().await?;

    let database = Database::open(path.clone()).await?;
    assert_eq!(database.nonterminal().await?.len(), 1);
    let source = engine_starting();
    database.close().await?;

    let database = Database::open(path.clone()).await?;
    let committed = database.append(source.clone()).await?;
    database.close().await?;

    let database = Database::open(path.clone()).await?;
    let duplicate = database.append(source).await?;
    assert!(committed.inserted);
    assert!(!duplicate.inserted);
    assert_eq!(
        database
            .events_after("kill-task".to_owned(), 0)
            .await?
            .len(),
        2
    );
    let finalization = database
        .finalize(terminal(), receipt(), "poll".to_owned())
        .await?;
    assert!(finalization.append.projection.status.is_terminal());
    let first_delivery = database.pending("play".to_owned()).await?;
    database.close().await?;

    let database = Database::open(path.clone()).await?;
    let retried_delivery = database.pending("play".to_owned()).await?;
    assert_eq!(first_delivery, retried_delivery);
    let rebuilt = database.rebuild("kill-task".to_owned()).await?;
    assert_eq!(rebuilt.event_seq, 3);
    database.close().await?;

    assert_eq!(
        transition(None, EffectState::Planned)?,
        EffectState::Planned
    );
    assert_eq!(
        transition(Some(EffectState::Started), EffectState::Uncertain)?,
        EffectState::Uncertain
    );
    remove_database_files(&path)?;
    Ok(())
}

fn engine_starting() -> EventInput {
    input(
        "engine.starting",
        "engine/start",
        "engine-source",
        serde_json::json!({}),
    )
}

fn terminal() -> EventInput {
    input(
        "turn.completed",
        "turn/completed",
        "terminal-source",
        serde_json::json!({"status":"completed"}),
    )
}

fn input(kind: &str, method: &str, key: &str, data: serde_json::Value) -> EventInput {
    EventInput {
        task_id: "kill-task".to_owned(),
        attempt: 1,
        kind: kind.to_owned(),
        data,
        source: Some(EventSource {
            engine: "fake".to_owned(),
            method: method.to_owned(),
            thread_id: Some("thread".to_owned()),
            turn_id: Some("turn".to_owned()),
            item_id: None,
            payload_hash: key.to_owned(),
        }),
        source_key: Some(key.to_owned()),
        observed_at: "2026-08-28T00:00:01Z".to_owned(),
    }
}

fn receipt() -> Receipt {
    Receipt {
        protocol_version: "0.1".to_owned(),
        receipt_id: "kill-receipt".to_owned(),
        task_id: "kill-task".to_owned(),
        attempt: 1,
        status: ReceiptStatus::Completed,
        summary: "done".to_owned(),
        artifacts: Vec::new(),
        verification: Vec::new(),
        verification_waiver: Some("kill fixture".to_owned()),
        usage: Usage::default(),
        engine: ReceiptEngine {
            kind: "fake".to_owned(),
            requested_model: "fake-local".to_owned(),
            observed_models: vec!["fake-local".to_owned()],
            version: Some("1".to_owned()),
        },
        final_event_seq: 3,
        completed_at: "2026-08-28T00:00:02Z".to_owned(),
    }
}

fn database_path() -> Result<PathBuf, Box<dyn std::error::Error>> {
    let nanos = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
    Ok(std::env::temp_dir().join(format!("spewer-kill-{}-{nanos}.sqlite", std::process::id())))
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
