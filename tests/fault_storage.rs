//! Restart and projection-corruption fault tests.

use rusqlite::{Connection, params};
use serde_json::json;
use spewer::protocol::{EventSource, TaskRequest};
use spewer::store::{Database, EventInput};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

#[tokio::test(flavor = "current_thread")]
async fn restart_deduplicates_committed_source_and_rebuilds_projection()
-> Result<(), Box<dyn std::error::Error>> {
    let path = database_path()?;
    let request: TaskRequest = serde_json::from_str(include_str!("fixtures/task-request.json"))?;
    let database = Database::open(path.clone()).await?;
    let accepted = database
        .accept(
            request,
            "restart-task".to_owned(),
            "2026-08-28T00:00:00Z".to_owned(),
        )
        .await?;
    let input = source_input();
    let committed = database.append(input.clone()).await?;
    database.close().await?;

    let reopened = Database::open(path.clone()).await?;
    let duplicate = reopened.append(input).await?;
    let before_corruption = reopened
        .get("restart-task".to_owned())
        .await?
        .ok_or("projection missing")?;
    reopened.close().await?;
    corrupt_projection(&path)?;

    let recovered = Database::open(path.clone()).await?;
    let rebuilt = recovered.rebuild("restart-task".to_owned()).await?;
    let events = recovered.events_after("restart-task".to_owned(), 0).await?;
    recovered.close().await?;

    assert!(accepted.created);
    assert!(committed.inserted);
    assert!(!duplicate.inserted);
    assert_eq!(
        serde_json::to_vec(&rebuilt)?,
        serde_json::to_vec(&before_corruption)?
    );
    assert_eq!(events.len(), 2);
    remove_database_files(&path)?;
    Ok(())
}

fn source_input() -> EventInput {
    EventInput {
        task_id: "restart-task".to_owned(),
        attempt: 1,
        kind: "engine.starting".to_owned(),
        data: json!({}),
        source: Some(EventSource {
            engine: "fake".to_owned(),
            method: "engine/start".to_owned(),
            thread_id: None,
            turn_id: None,
            item_id: None,
            payload_hash: "payload".to_owned(),
        }),
        source_key: Some("engine/start:payload:1".to_owned()),
        observed_at: "2026-08-28T00:00:01Z".to_owned(),
    }
}

fn corrupt_projection(path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let connection = Connection::open(path)?;
    connection.execute(
        "UPDATE tasks SET projection_json = ?1 WHERE task_id = ?2",
        params!["{}", "restart-task"],
    )?;
    Ok(())
}

fn database_path() -> Result<PathBuf, Box<dyn std::error::Error>> {
    let nanos = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
    Ok(std::env::temp_dir().join(format!(
        "spewer-fault-{}-{nanos}.sqlite",
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
