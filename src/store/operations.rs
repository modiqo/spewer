use super::{AcceptedTask, AppendOutcome, EventInput, Observation};
use crate::error::{Error, ErrorKind, Result};
use crate::protocol::{Event, PROTOCOL_VERSION, TaskHandle, TaskRequest, TaskStatus};
use crate::reducer::{Projection, apply};
use crate::util::request_hash;
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};

pub(super) fn accept(
    connection: &mut Connection,
    request: &TaskRequest,
    task_id: &str,
    created_at: &str,
) -> Result<AcceptedTask> {
    let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let submitted_hash = request_hash(request)?;
    let existing = transaction
        .query_row(
            "SELECT projection_json, request_json, request_hash FROM tasks WHERE idempotency_key = ?1",
            params![request.idempotency_key],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                ))
            },
        )
        .optional()?;
    if let Some((projection_json, request_json, stored_hash)) = existing {
        let effective_hash = if stored_hash.is_empty() {
            let stored: TaskRequest = serde_json::from_str(&request_json)?;
            let hash = request_hash(&stored)?;
            transaction.execute(
                "UPDATE tasks SET request_hash = ?2 WHERE idempotency_key = ?1",
                params![request.idempotency_key, hash],
            )?;
            hash
        } else {
            stored_hash
        };
        if effective_hash != submitted_hash {
            return Err(Error::new(
                ErrorKind::InvalidInput,
                "idempotency key is already bound to a different task request",
            ));
        }
        let projection: Projection = serde_json::from_str(&projection_json)?;
        let handle = handle_from_projection(&projection);
        transaction.commit()?;
        return Ok(AcceptedTask {
            handle,
            projection,
            event: None,
            created: false,
        });
    }
    let initial = Projection::initial(task_id.to_owned(), request, created_at.to_owned());
    let event = Event {
        protocol_version: PROTOCOL_VERSION.to_owned(),
        task_id: task_id.to_owned(),
        attempt: 1,
        seq: 1,
        kind: "task.accepted".to_owned(),
        observed_at: created_at.to_owned(),
        data: serde_json::json!({"idempotency_key": request.idempotency_key}),
        source: None,
    };
    let projection = apply(&initial, &event)?;
    transaction.execute(
        "INSERT INTO tasks(task_id, idempotency_key, request_json, projection_json, status, event_seq, created_at, updated_at, request_hash)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?7, ?8)",
        params![
            task_id,
            request.idempotency_key,
            serde_json::to_string(request)?,
            serde_json::to_string(&projection)?,
            status_name(projection.status),
            to_i64(projection.event_seq)?,
            created_at,
            submitted_hash,
        ],
    )?;
    transaction.execute(
        "INSERT INTO attempts(task_id, attempt, engine_kind) VALUES (?1, 1, ?2)",
        params![projection.task_id, request.engine.kind],
    )?;
    insert_event(&transaction, &event, None)?;
    transaction.execute(
        "INSERT INTO dispatches(task_id, state, updated_at) VALUES (?1, 'queued', ?2)",
        params![task_id, created_at],
    )?;
    let handle = handle_from_projection(&projection);
    transaction.commit()?;
    Ok(AcceptedTask {
        handle,
        projection,
        event: Some(event),
        created: true,
    })
}

pub(super) fn append(connection: &mut Connection, input: EventInput) -> Result<AppendOutcome> {
    let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let outcome = append_in(&transaction, input)?;
    transaction.commit()?;
    Ok(outcome)
}

pub(super) fn append_in(
    transaction: &rusqlite::Transaction<'_>,
    input: EventInput,
) -> Result<AppendOutcome> {
    let projection_json: String = transaction
        .query_row(
            "SELECT projection_json FROM tasks WHERE task_id = ?1",
            params![input.task_id],
            |row| row.get(0),
        )
        .optional()?
        .ok_or_else(|| Error::new(ErrorKind::InvalidInput, "task does not exist"))?;
    let current: Projection = serde_json::from_str(&projection_json)?;
    if let Some(source_key) = &input.source_key {
        let duplicate = transaction
            .query_row(
                "SELECT event_json FROM events WHERE task_id = ?1 AND source_key = ?2",
                params![input.task_id, source_key],
                |row| row.get::<_, String>(0),
            )
            .optional()?;
        if let Some(json) = duplicate {
            let event = serde_json::from_str(&json)?;
            return Ok(AppendOutcome {
                event,
                projection: current,
                inserted: false,
            });
        }
    }
    let seq = current
        .event_seq
        .checked_add(1)
        .ok_or_else(|| Error::new(ErrorKind::InvalidInput, "event sequence exhausted"))?;
    let event = Event {
        protocol_version: PROTOCOL_VERSION.to_owned(),
        task_id: input.task_id,
        attempt: input.attempt,
        seq,
        kind: input.kind,
        observed_at: input.observed_at,
        data: input.data,
        source: input.source,
    };
    let projection = apply(&current, &event)?;
    insert_event(transaction, &event, input.source_key.as_deref())?;
    if let Some(source_key) = input.source_key {
        let source = event.source.as_ref().ok_or_else(|| {
            Error::new(
                ErrorKind::InvalidInput,
                "source key requires source metadata",
            )
        })?;
        transaction.execute(
            "INSERT INTO source_events(task_id, source_key, seq, method, payload_hash)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                event.task_id,
                source_key,
                to_i64(seq)?,
                source.method,
                source.payload_hash
            ],
        )?;
    }
    transaction.execute(
        "UPDATE tasks SET projection_json = ?2, status = ?3, event_seq = ?4, updated_at = ?5
         WHERE task_id = ?1",
        params![
            event.task_id,
            serde_json::to_string(&projection)?,
            status_name(projection.status),
            to_i64(projection.event_seq)?,
            event.observed_at,
        ],
    )?;
    Ok(AppendOutcome {
        event,
        projection,
        inserted: true,
    })
}

pub(super) fn get(connection: &Connection, task_id: &str) -> Result<Option<Projection>> {
    let json = connection
        .query_row(
            "SELECT projection_json FROM tasks WHERE task_id = ?1",
            params![task_id],
            |row| row.get::<_, String>(0),
        )
        .optional()?;
    json.map(|value| serde_json::from_str(&value).map_err(Error::from))
        .transpose()
}

pub(super) fn request(connection: &Connection, task_id: &str) -> Result<TaskRequest> {
    let json = connection
        .query_row(
            "SELECT request_json FROM tasks WHERE task_id = ?1",
            params![task_id],
            |row| row.get::<_, String>(0),
        )
        .optional()?
        .ok_or_else(|| Error::new(ErrorKind::InvalidInput, "task does not exist"))?;
    Ok(serde_json::from_str(&json)?)
}

pub(super) fn events_after(
    connection: &Connection,
    task_id: &str,
    after: u64,
) -> Result<Vec<Event>> {
    let mut statement = connection
        .prepare("SELECT event_json FROM events WHERE task_id = ?1 AND seq > ?2 ORDER BY seq")?;
    let rows = statement.query_map(params![task_id, to_i64(after)?], |row| {
        row.get::<_, String>(0)
    })?;
    let mut events = Vec::new();
    for row in rows {
        events.push(serde_json::from_str(&row?)?);
    }
    Ok(events)
}

pub(super) fn observe(connection: &Connection, task_id: &str, after: u64) -> Result<Observation> {
    let projection = get(connection, task_id)?
        .ok_or_else(|| Error::new(ErrorKind::InvalidInput, "task does not exist"))?;
    let events = events_after(connection, task_id, after)?;
    let next_cursor = projection.event_seq;
    let poll_after_ms = if projection.status.is_terminal() {
        0
    } else if projection.status == TaskStatus::Queued {
        250
    } else {
        500
    };
    Ok(Observation {
        projection,
        events,
        next_cursor,
        poll_after_ms,
    })
}

pub(super) fn rebuild(connection: &mut Connection, task_id: &str) -> Result<Projection> {
    let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let (request_json, created_at): (String, String) = transaction
        .query_row(
            "SELECT request_json, created_at FROM tasks WHERE task_id = ?1",
            params![task_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?
        .ok_or_else(|| Error::new(ErrorKind::InvalidInput, "task does not exist"))?;
    let request: TaskRequest = serde_json::from_str(&request_json)?;
    let mut projection = Projection::initial(task_id.to_owned(), &request, created_at);
    {
        let mut statement =
            transaction.prepare("SELECT event_json FROM events WHERE task_id = ?1 ORDER BY seq")?;
        let rows = statement.query_map(params![task_id], |row| row.get::<_, String>(0))?;
        for row in rows {
            let event: Event = serde_json::from_str(&row?)?;
            projection = apply(&projection, &event)?;
        }
    }
    transaction.execute(
        "UPDATE tasks SET projection_json = ?2, status = ?3, event_seq = ?4 WHERE task_id = ?1",
        params![
            task_id,
            serde_json::to_string(&projection)?,
            status_name(projection.status),
            to_i64(projection.event_seq)?,
        ],
    )?;
    transaction.commit()?;
    Ok(projection)
}

fn insert_event(
    transaction: &rusqlite::Transaction<'_>,
    event: &Event,
    source_key: Option<&str>,
) -> Result<()> {
    transaction.execute(
        "INSERT INTO events(task_id, seq, attempt, event_type, observed_at, event_json, source_key)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![
            event.task_id,
            to_i64(event.seq)?,
            i64::from(event.attempt),
            event.kind,
            event.observed_at,
            serde_json::to_string(event)?,
            source_key,
        ],
    )?;
    Ok(())
}

fn handle_from_projection(projection: &Projection) -> TaskHandle {
    TaskHandle {
        protocol_version: PROTOCOL_VERSION.to_owned(),
        task_id: projection.task_id.clone(),
        status: projection.status,
        event_cursor: projection.event_seq,
        created_at: projection.created_at.clone(),
    }
}

const fn status_name(status: TaskStatus) -> &'static str {
    match status {
        TaskStatus::Queued => "queued",
        TaskStatus::Starting => "starting",
        TaskStatus::Running => "running",
        TaskStatus::InputRequired => "input_required",
        TaskStatus::Stalled => "stalled",
        TaskStatus::Completed => "completed",
        TaskStatus::Failed => "failed",
        TaskStatus::Cancelled => "cancelled",
        TaskStatus::Escalated => "escalated",
    }
}

fn to_i64(value: u64) -> Result<i64> {
    i64::try_from(value).map_err(|error| Error::new(ErrorKind::InvalidInput, error.to_string()))
}
