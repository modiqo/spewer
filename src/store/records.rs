use super::{AppendOutcome, CancelOutcome, EventInput, TaskResult};
use crate::delivery::OutboxMessage;
use crate::error::{Error, ErrorKind, Result};
use crate::protocol::{Checkpoint, Receipt};
use crate::reducer::Projection;
use crate::util::{new_id, now};
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};

pub(super) fn save_checkpoint(connection: &Connection, value: &Checkpoint) -> Result<()> {
    connection.execute(
        "INSERT OR IGNORE INTO checkpoints(checkpoint_id, task_id, attempt, event_seq, resumable, checkpoint_json, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![
            value.checkpoint_id,
            value.task_id,
            i64::from(value.attempt),
            i64::try_from(value.event_seq).map_err(|error| Error::new(ErrorKind::Storage, error.to_string()))?,
            value.resumable,
            serde_json::to_string(value)?,
            value.created_at,
        ],
    )?;
    Ok(())
}

pub(super) fn latest_checkpoint(
    connection: &Connection,
    task_id: &str,
) -> Result<Option<Checkpoint>> {
    let json = connection
        .query_row(
            "SELECT checkpoint_json FROM checkpoints WHERE task_id = ?1 ORDER BY event_seq DESC LIMIT 1",
            params![task_id],
            |row| row.get::<_, String>(0),
        )
        .optional()?;
    json.map(|value| serde_json::from_str(&value).map_err(Error::from))
        .transpose()
}

pub(super) fn nonterminal(connection: &Connection) -> Result<Vec<Projection>> {
    let mut statement = connection.prepare(
        "SELECT projection_json FROM tasks WHERE status NOT IN ('completed','failed','cancelled','escalated') ORDER BY updated_at",
    )?;
    let rows = statement.query_map([], |row| row.get::<_, String>(0))?;
    let mut projections = Vec::new();
    for row in rows {
        projections.push(serde_json::from_str(&row?)?);
    }
    Ok(projections)
}

pub(super) fn commit_receipt(
    connection: &mut Connection,
    receipt: &Receipt,
    mode: &str,
) -> Result<OutboxMessage> {
    let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let message = insert_receipt_message(&transaction, receipt, mode)?;
    super::dispatch::complete(&transaction, &receipt.task_id)?;
    transaction.commit()?;
    Ok(message)
}

pub(super) fn finalize(
    connection: &mut Connection,
    input: EventInput,
    receipt: &Receipt,
    mode: &str,
) -> Result<(AppendOutcome, OutboxMessage)> {
    let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let append = super::operations::append_in(&transaction, input)?;
    if append.projection.event_seq != receipt.final_event_seq {
        return Err(Error::new(
            ErrorKind::InvalidInput,
            "receipt cursor does not match terminal projection",
        ));
    }
    let message = insert_receipt_message(&transaction, receipt, mode)?;
    transaction.commit()?;
    Ok((append, message))
}

pub(super) fn insert_receipt_message(
    transaction: &rusqlite::Transaction<'_>,
    receipt: &Receipt,
    mode: &str,
) -> Result<OutboxMessage> {
    if let Some(json) = transaction
        .query_row(
            "SELECT payload_json FROM outbox WHERE task_id = ?1",
            params![receipt.task_id],
            |row| row.get::<_, String>(0),
        )
        .optional()?
    {
        return Ok(serde_json::from_str(&json)?);
    }
    let message = OutboxMessage {
        message_id: new_id("msg")?,
        task_id: receipt.task_id.clone(),
        receipt_id: receipt.receipt_id.clone(),
        mode: mode.to_owned(),
        receipt: receipt.clone(),
        created_at: now()?,
    };
    transaction.execute(
        "INSERT INTO receipts(receipt_id, task_id, receipt_json, created_at) VALUES (?1, ?2, ?3, ?4)",
        params![receipt.receipt_id, receipt.task_id, serde_json::to_string(receipt)?, receipt.completed_at],
    )?;
    transaction.execute(
        "INSERT INTO outbox(message_id, task_id, receipt_id, mode, payload_json, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![message.message_id, message.task_id, message.receipt_id, message.mode, serde_json::to_string(&message)?, message.created_at],
    )?;
    Ok(message)
}

pub(super) fn pending(connection: &Connection, consumer_id: &str) -> Result<Vec<OutboxMessage>> {
    let mut statement = connection.prepare(
        "SELECT o.payload_json, t.request_json
           FROM outbox o
           JOIN tasks t ON t.task_id = o.task_id
           LEFT JOIN deliveries d
             ON o.message_id = d.message_id AND d.consumer_id = ?1
          WHERE d.message_id IS NULL
          ORDER BY o.created_at",
    )?;
    let rows = statement.query_map(params![consumer_id], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
    })?;
    let mut messages = Vec::new();
    for row in rows {
        let (message_json, request_json) = row?;
        let request: crate::protocol::TaskRequest = serde_json::from_str(&request_json)?;
        if request.callback.consumer_id.as_deref() == Some(consumer_id) {
            messages.push(serde_json::from_str(&message_json)?);
        }
    }
    Ok(messages)
}

pub(super) fn result(connection: &Connection, task_id: &str) -> Result<TaskResult> {
    let projection = super::operations::get(connection, task_id)?
        .ok_or_else(|| Error::new(ErrorKind::InvalidInput, "task does not exist"))?;
    let json = connection
        .query_row(
            "SELECT payload_json FROM outbox WHERE task_id = ?1",
            params![task_id],
            |row| row.get::<_, String>(0),
        )
        .optional()?;
    let message = json
        .map(|value| serde_json::from_str(&value).map_err(Error::from))
        .transpose()?;
    Ok(TaskResult {
        projection,
        message,
    })
}

pub(super) fn cancel(
    connection: &mut Connection,
    task_id: &str,
    reason: &str,
) -> Result<CancelOutcome> {
    let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let (projection_json, request_json): (String, String) = transaction
        .query_row(
            "SELECT projection_json, request_json FROM tasks WHERE task_id = ?1",
            params![task_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?
        .ok_or_else(|| Error::new(ErrorKind::InvalidInput, "task does not exist"))?;
    let projection: Projection = serde_json::from_str(&projection_json)?;
    if projection.status.is_terminal() {
        transaction.commit()?;
        return Ok(CancelOutcome {
            projection,
            message: None,
            changed: false,
        });
    }
    let request = serde_json::from_str::<crate::protocol::TaskRequest>(&request_json)?;
    let appended = super::operations::append_in(
        &transaction,
        EventInput {
            task_id: task_id.to_owned(),
            attempt: projection.attempt,
            kind: "task.cancelled".to_owned(),
            data: serde_json::json!({"reason": reason}),
            source: None,
            source_key: None,
            observed_at: now()?,
        },
    )?;
    let receipt = crate::receipt::build_cancelled_receipt(&appended.projection, &request)?;
    let message = insert_receipt_message(&transaction, &receipt, &request.callback.mode)?;
    super::dispatch::complete(&transaction, task_id)?;
    transaction.commit()?;
    Ok(CancelOutcome {
        projection: appended.projection,
        message: Some(message),
        changed: true,
    })
}

pub(super) fn acknowledge(
    connection: &Connection,
    message_id: &str,
    consumer_id: &str,
) -> Result<bool> {
    let request_json = connection
        .query_row(
            "SELECT t.request_json
               FROM outbox o JOIN tasks t ON t.task_id = o.task_id
              WHERE o.message_id = ?1",
            params![message_id],
            |row| row.get::<_, String>(0),
        )
        .optional()?
        .ok_or_else(|| Error::new(ErrorKind::InvalidInput, "outbox message does not exist"))?;
    let request: crate::protocol::TaskRequest = serde_json::from_str(&request_json)?;
    match request.callback.consumer_id.as_deref() {
        Some(expected) if expected == consumer_id => {}
        Some(_) => {
            return Err(Error::new(
                ErrorKind::InvalidInput,
                "consumer is not authorized to acknowledge this message",
            ));
        }
        None => {
            return Err(Error::new(
                ErrorKind::InvalidInput,
                "task has no acknowledgement consumer",
            ));
        }
    }
    let changed = connection.execute(
        "INSERT OR IGNORE INTO deliveries(message_id, consumer_id, acknowledged_at) VALUES (?1, ?2, ?3)",
        params![message_id, consumer_id, now()?],
    )?;
    Ok(changed == 1)
}

pub(super) fn reconcile_uncertain(
    connection: &mut Connection,
    task_id: &str,
    reason: &str,
) -> Result<()> {
    let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let (projection_json, request_json): (String, String) = transaction
        .query_row(
            "SELECT projection_json, request_json FROM tasks WHERE task_id = ?1",
            params![task_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?
        .ok_or_else(|| Error::new(ErrorKind::InvalidInput, "task does not exist"))?;
    let current: Projection = serde_json::from_str(&projection_json)?;
    if current.status.is_terminal() {
        super::dispatch::complete(&transaction, task_id)?;
        transaction.commit()?;
        return Ok(());
    }
    let request: crate::protocol::TaskRequest = serde_json::from_str(&request_json)?;
    let append = super::operations::append_in(
        &transaction,
        EventInput {
            task_id: task_id.to_owned(),
            attempt: current.attempt,
            kind: "task.escalated".to_owned(),
            data: serde_json::json!({"reason": reason}),
            source: None,
            source_key: None,
            observed_at: now()?,
        },
    )?;
    let receipt = crate::receipt::build_escalated_receipt(&append.projection, &request)?;
    let _message = insert_receipt_message(&transaction, &receipt, &request.callback.mode)?;
    super::dispatch::complete(&transaction, task_id)?;
    transaction.commit()?;
    Ok(())
}
