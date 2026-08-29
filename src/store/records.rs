use super::{AppendOutcome, EventInput};
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

fn insert_receipt_message(
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
        "SELECT o.payload_json FROM outbox o LEFT JOIN deliveries d ON o.message_id = d.message_id AND d.consumer_id = ?1 WHERE d.message_id IS NULL ORDER BY o.created_at",
    )?;
    let rows = statement.query_map(params![consumer_id], |row| row.get::<_, String>(0))?;
    let mut messages = Vec::new();
    for row in rows {
        messages.push(serde_json::from_str(&row?)?);
    }
    Ok(messages)
}

pub(super) fn acknowledge(
    connection: &Connection,
    message_id: &str,
    consumer_id: &str,
) -> Result<bool> {
    let changed = connection.execute(
        "INSERT OR IGNORE INTO deliveries(message_id, consumer_id, acknowledged_at) VALUES (?1, ?2, ?3)",
        params![message_id, consumer_id, now()?],
    )?;
    Ok(changed == 1)
}
