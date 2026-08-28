//! Durable event transaction and replay tests.

use serde_json::json;
use spewer::protocol::TaskRequest;
use spewer::store::{Database, EventInput};
use std::path::Path;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

fn database_path() -> Result<PathBuf, Box<dyn std::error::Error>> {
    let nanos = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
    Ok(std::env::temp_dir().join(format!(
        "spewer-store-{}-{nanos}.sqlite",
        std::process::id()
    )))
}

#[tokio::test(flavor = "current_thread")]
async fn acceptance_append_dedup_and_replay_are_atomic() -> Result<(), Box<dyn std::error::Error>> {
    let path = database_path()?;
    let database = Database::open(path.clone()).await?;
    let request: TaskRequest = serde_json::from_str(include_str!("fixtures/task-request.json"))?;
    let accepted = database
        .accept(
            request.clone(),
            "task-one".to_owned(),
            "2026-08-28T00:00:00Z".to_owned(),
        )
        .await?;
    let duplicate_accept = database
        .accept(
            request,
            "task-two".to_owned(),
            "2026-08-28T00:00:01Z".to_owned(),
        )
        .await?;
    let input = EventInput {
        task_id: "task-one".to_owned(),
        attempt: 1,
        kind: "engine.starting".to_owned(),
        data: json!({}),
        source: Some(spewer::protocol::EventSource {
            engine: "fake".to_owned(),
            method: "start".to_owned(),
            thread_id: None,
            turn_id: None,
            item_id: None,
            payload_hash: "hash".to_owned(),
        }),
        source_key: Some("source-one".to_owned()),
        observed_at: "2026-08-28T00:00:02Z".to_owned(),
    };
    let appended = database.append(input.clone()).await?;
    let duplicate = database.append(input).await?;
    let events = database.events_after("task-one".to_owned(), 0).await?;
    let rebuilt = database.rebuild("task-one".to_owned()).await?;
    let current = database
        .get("task-one".to_owned())
        .await?
        .ok_or("task disappeared")?;
    database.close().await?;

    assert!(accepted.created);
    assert!(!duplicate_accept.created);
    assert_eq!(duplicate_accept.handle.task_id, "task-one");
    assert!(appended.inserted);
    assert!(!duplicate.inserted);
    assert_eq!(events.len(), 2);
    assert_eq!(rebuilt, current);

    let reopened = Database::open(path.clone()).await?;
    let recovered = reopened.get("task-one".to_owned()).await?;
    reopened.close().await?;
    assert_eq!(recovered, Some(current));
    remove_database_files(&path)?;
    Ok(())
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
