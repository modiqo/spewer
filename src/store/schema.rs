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
    migrate_recovery(connection)?;
    Ok(())
}

fn migrate_recovery(connection: &Connection) -> Result<()> {
    connection.execute_batch(
        r"
        CREATE TABLE IF NOT EXISTS checkpoints (
            checkpoint_id TEXT PRIMARY KEY,
            task_id TEXT NOT NULL,
            attempt INTEGER NOT NULL,
            event_seq INTEGER NOT NULL,
            resumable INTEGER NOT NULL,
            checkpoint_json TEXT NOT NULL,
            created_at TEXT NOT NULL,
            FOREIGN KEY (task_id) REFERENCES tasks(task_id)
        );

        CREATE TABLE IF NOT EXISTS receipts (
            receipt_id TEXT PRIMARY KEY,
            task_id TEXT NOT NULL UNIQUE,
            receipt_json TEXT NOT NULL,
            created_at TEXT NOT NULL,
            FOREIGN KEY (task_id) REFERENCES tasks(task_id)
        );

        CREATE TABLE IF NOT EXISTS outbox (
            message_id TEXT PRIMARY KEY,
            task_id TEXT NOT NULL UNIQUE,
            receipt_id TEXT NOT NULL,
            mode TEXT NOT NULL,
            payload_json TEXT NOT NULL,
            created_at TEXT NOT NULL,
            FOREIGN KEY (task_id) REFERENCES tasks(task_id),
            FOREIGN KEY (receipt_id) REFERENCES receipts(receipt_id)
        );

        CREATE TABLE IF NOT EXISTS deliveries (
            message_id TEXT NOT NULL,
            consumer_id TEXT NOT NULL,
            acknowledged_at TEXT NOT NULL,
            PRIMARY KEY (message_id, consumer_id),
            FOREIGN KEY (message_id) REFERENCES outbox(message_id)
        );

        CREATE TABLE IF NOT EXISTS side_effects (
            effect_key TEXT PRIMARY KEY,
            task_id TEXT NOT NULL,
            status TEXT NOT NULL,
            evidence_json TEXT,
            updated_at TEXT NOT NULL,
            FOREIGN KEY (task_id) REFERENCES tasks(task_id)
        );

        CREATE INDEX IF NOT EXISTS checkpoints_task
            ON checkpoints(task_id, event_seq DESC);
        CREATE INDEX IF NOT EXISTS outbox_task
            ON outbox(task_id, created_at);

        INSERT OR IGNORE INTO schema_migrations(version, applied_at)
            VALUES (2, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'));
        ",
    )?;
    Ok(())
}
