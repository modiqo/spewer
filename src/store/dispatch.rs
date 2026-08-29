//! Durable queue intent, worker leases, and startup reconciliation.

use super::{AppendOutcome, EventInput};
use crate::error::{Error, ErrorKind, Result};
use crate::protocol::{TaskRequest, TaskStatus};
use crate::reducer::Projection;
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};

/// One task that can be scheduled without repeating observable engine work.
#[derive(Clone, Debug)]
pub struct RecoveryJob {
    /// Durable task identity.
    pub task_id: String,
    /// Immutable request accepted for the task.
    pub request: TaskRequest,
}

/// One previous worker whose outcome requires conservative reconciliation.
#[derive(Clone, Debug)]
pub struct UncertainDispatch {
    /// Durable task identity.
    pub task_id: String,
    /// Immutable request accepted for the task.
    pub request: TaskRequest,
    /// Last durable lease identity, when one existed.
    pub lease_id: Option<String>,
    /// App Server process group recorded before initialization.
    pub process_group: Option<u32>,
    /// Executable signature recorded with the process group.
    pub process_signature: Option<String>,
    /// Whether the task already has an immutable terminal result.
    pub terminal: bool,
}

/// Startup work reconstructed from the durable dispatch ledger.
#[derive(Clone, Debug, Default)]
pub struct RecoveryBatch {
    /// Tasks safe to place back in the in-memory scheduling cache.
    pub runnable: Vec<RecoveryJob>,
    /// Tasks or processes that need fail-closed reconciliation.
    pub uncertain: Vec<UncertainDispatch>,
}

#[derive(Debug)]
struct DispatchRow {
    task_id: String,
    state: String,
    request: TaskRequest,
    projection: Projection,
    lease_id: Option<String>,
    process_group: Option<u32>,
    process_signature: Option<String>,
}

pub(super) fn lease(
    connection: &mut Connection,
    input: EventInput,
    lease_id: &str,
    server_epoch: &str,
    worker_id: &str,
    expires_at: &str,
) -> Result<AppendOutcome> {
    let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let changed = transaction.execute(
        "UPDATE dispatches
            SET state = 'leased', lease_id = ?2, server_epoch = ?3, worker_id = ?4,
                acquired_at = ?5, expires_at = ?6, process_group = NULL,
                process_signature = NULL, process_started_at = NULL, updated_at = ?5
          WHERE task_id = ?1 AND state = 'queued'",
        params![
            input.task_id,
            lease_id,
            server_epoch,
            worker_id,
            input.observed_at,
            expires_at
        ],
    )?;
    if changed != 1 {
        return Err(Error::new(
            ErrorKind::InvalidInput,
            "task is not durably queued for leasing",
        ));
    }
    let outcome = super::operations::append_in(&transaction, input)?;
    transaction.commit()?;
    Ok(outcome)
}

pub(super) fn register_process(
    connection: &Connection,
    task_id: &str,
    lease_id: &str,
    process_group: u32,
    process_signature: &str,
    started_at: &str,
) -> Result<()> {
    let changed = connection.execute(
        "UPDATE dispatches
            SET process_group = ?3, process_signature = ?4,
                process_started_at = ?5, updated_at = ?5
          WHERE task_id = ?1 AND lease_id = ?2 AND state = 'leased'",
        params![
            task_id,
            lease_id,
            i64::from(process_group),
            process_signature,
            started_at
        ],
    )?;
    if changed != 1 {
        return Err(Error::new(
            ErrorKind::InvalidInput,
            "worker process does not belong to the active durable lease",
        ));
    }
    Ok(())
}

pub(super) fn complete(connection: &Connection, task_id: &str) -> Result<()> {
    connection.execute(
        "UPDATE dispatches
            SET state = 'terminal', lease_id = NULL, server_epoch = NULL,
                worker_id = NULL, acquired_at = NULL, expires_at = NULL,
                process_group = NULL, process_signature = NULL,
                process_started_at = NULL,
                updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
          WHERE task_id = ?1",
        params![task_id],
    )?;
    Ok(())
}

pub(super) fn startup(connection: &mut Connection) -> Result<RecoveryBatch> {
    let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let rows = load_rows(&transaction)?;
    let mut batch = RecoveryBatch::default();
    for row in rows {
        let terminal = row.projection.status.is_terminal();
        let has_process = row.process_group.is_some();
        if terminal {
            if has_process {
                batch.uncertain.push(uncertain(row, true));
            } else {
                mark_terminal(&transaction, &row.task_id)?;
            }
            continue;
        }
        let pristine = row.projection.status == TaskStatus::Queued
            && row.projection.workspace.path.is_empty()
            && !has_process;
        if pristine && matches!(row.state.as_str(), "queued" | "leased") {
            mark_queued(&transaction, &row.task_id)?;
            batch.runnable.push(RecoveryJob {
                task_id: row.task_id,
                request: row.request,
            });
        } else {
            mark_uncertain(&transaction, &row.task_id)?;
            batch.uncertain.push(uncertain(row, false));
        }
    }
    transaction.commit()?;
    Ok(batch)
}

fn load_rows(transaction: &rusqlite::Transaction<'_>) -> Result<Vec<DispatchRow>> {
    let mut statement = transaction.prepare(
        "SELECT d.task_id, d.state, t.request_json, t.projection_json,
                d.lease_id, d.process_group, d.process_signature
           FROM dispatches d JOIN tasks t ON t.task_id = d.task_id
          ORDER BY t.created_at, d.task_id",
    )?;
    let mapped = statement.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, String>(3)?,
            row.get::<_, Option<String>>(4)?,
            row.get::<_, Option<i64>>(5)?,
            row.get::<_, Option<String>>(6)?,
        ))
    })?;
    let mut rows = Vec::new();
    for mapped in mapped {
        let (task_id, state, request, projection, lease_id, process_group, signature) = mapped?;
        let process_group = process_group
            .map(u32::try_from)
            .transpose()
            .map_err(|error| Error::new(ErrorKind::Storage, error.to_string()))?;
        rows.push(DispatchRow {
            task_id,
            state,
            request: serde_json::from_str(&request)?,
            projection: serde_json::from_str(&projection)?,
            lease_id,
            process_group,
            process_signature: signature,
        });
    }
    Ok(rows)
}

fn uncertain(row: DispatchRow, terminal: bool) -> UncertainDispatch {
    UncertainDispatch {
        task_id: row.task_id,
        request: row.request,
        lease_id: row.lease_id,
        process_group: row.process_group,
        process_signature: row.process_signature,
        terminal,
    }
}

fn mark_queued(transaction: &rusqlite::Transaction<'_>, task_id: &str) -> Result<()> {
    transaction.execute(
        "UPDATE dispatches
            SET state = 'queued', lease_id = NULL, server_epoch = NULL,
                worker_id = NULL, acquired_at = NULL, expires_at = NULL,
                updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
          WHERE task_id = ?1",
        params![task_id],
    )?;
    Ok(())
}

fn mark_uncertain(transaction: &rusqlite::Transaction<'_>, task_id: &str) -> Result<()> {
    transaction.execute(
        "UPDATE dispatches SET state = 'uncertain',
                updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
          WHERE task_id = ?1",
        params![task_id],
    )?;
    Ok(())
}

fn mark_terminal(transaction: &rusqlite::Transaction<'_>, task_id: &str) -> Result<()> {
    transaction.execute(
        "UPDATE dispatches SET state = 'terminal', process_group = NULL,
                process_signature = NULL, process_started_at = NULL,
                updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
          WHERE task_id = ?1",
        params![task_id],
    )?;
    Ok(())
}

pub(super) fn state(connection: &Connection, task_id: &str) -> Result<Option<String>> {
    connection
        .query_row(
            "SELECT state FROM dispatches WHERE task_id = ?1",
            params![task_id],
            |row| row.get(0),
        )
        .optional()
        .map_err(Error::from)
}
