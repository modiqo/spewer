use crate::error::Result;
use rusqlite::Connection;

pub(super) fn migrate(connection: &Connection) -> Result<()> {
    connection.execute_batch(
        r"
        PRAGMA journal_mode = WAL;
        PRAGMA synchronous = FULL;
        PRAGMA foreign_keys = ON;
        PRAGMA busy_timeout = 5000;

        CREATE TABLE IF NOT EXISTS schema_migrations (
            version INTEGER PRIMARY KEY,
            applied_at TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS tasks (
            task_id TEXT PRIMARY KEY,
            idempotency_key TEXT NOT NULL UNIQUE,
            request_json TEXT NOT NULL,
            projection_json TEXT NOT NULL,
            status TEXT NOT NULL,
            event_seq INTEGER NOT NULL,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS attempts (
            task_id TEXT NOT NULL,
            attempt INTEGER NOT NULL,
            engine_kind TEXT NOT NULL,
            engine_handle_json TEXT,
            started_at TEXT,
            completed_at TEXT,
            PRIMARY KEY (task_id, attempt),
            FOREIGN KEY (task_id) REFERENCES tasks(task_id)
        );

        CREATE TABLE IF NOT EXISTS events (
            task_id TEXT NOT NULL,
            seq INTEGER NOT NULL,
            attempt INTEGER NOT NULL,
            event_type TEXT NOT NULL,
            observed_at TEXT NOT NULL,
            event_json TEXT NOT NULL,
            source_key TEXT,
            PRIMARY KEY (task_id, seq),
            UNIQUE (task_id, source_key),
            FOREIGN KEY (task_id) REFERENCES tasks(task_id)
        );

        CREATE TABLE IF NOT EXISTS source_events (
            task_id TEXT NOT NULL,
            source_key TEXT NOT NULL,
            seq INTEGER NOT NULL,
            method TEXT NOT NULL,
            payload_hash TEXT NOT NULL,
            PRIMARY KEY (task_id, source_key),
            FOREIGN KEY (task_id, seq) REFERENCES events(task_id, seq)
        );

        CREATE TABLE IF NOT EXISTS artifacts (
            artifact_id TEXT PRIMARY KEY,
            task_id TEXT NOT NULL,
            kind TEXT NOT NULL,
            uri TEXT NOT NULL,
            sha256 TEXT NOT NULL,
            metadata_json TEXT NOT NULL,
            created_at TEXT NOT NULL,
            FOREIGN KEY (task_id) REFERENCES tasks(task_id)
        );

        CREATE INDEX IF NOT EXISTS events_task_type
            ON events(task_id, event_type, seq);
        CREATE INDEX IF NOT EXISTS tasks_status
            ON tasks(status, updated_at);

        INSERT OR IGNORE INTO schema_migrations(version, applied_at)
            VALUES (1, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'));
        ",
    )?;
    Ok(())
}
