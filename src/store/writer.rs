//! Blocking `SQLite` writer loop and command dispatch.

use super::{Command, FinalizeOutcome, dispatch, operations, records, schema};
use crate::error::{Error, Result};
use rusqlite::Connection;
use std::path::PathBuf;
use tokio::sync::{mpsc, oneshot};

pub(super) fn run(
    path: PathBuf,
    mut receiver: mpsc::Receiver<Command>,
    ready: oneshot::Sender<Result<()>>,
) {
    let mut connection = match Connection::open(path) {
        Ok(connection) => connection,
        Err(error) => {
            let _sent = ready.send(Err(error.into()));
            return;
        }
    };
    if let Err(error) = schema::migrate(&connection) {
        let _sent = ready.send(Err(error));
        return;
    }
    let _sent = ready.send(Ok(()));
    while let Some(command) = receiver.blocking_recv() {
        if execute(command, &mut connection) {
            break;
        }
    }
}

#[allow(
    clippy::too_many_lines,
    reason = "one exhaustive dispatcher keeps every storage command visible in one place"
)]
fn execute(command: Command, connection: &mut Connection) -> bool {
    match command {
        Command::Accept {
            request,
            task_id,
            created_at,
            reply,
        } => {
            let result = operations::accept(connection, &request, &task_id, &created_at);
            let _sent = reply.send(result);
        }
        Command::Append { input, reply } => {
            let _sent = reply.send(operations::append(connection, *input));
        }
        Command::Get { task_id, reply } => {
            let _sent = reply.send(operations::get(connection, &task_id));
        }
        Command::Request { task_id, reply } => {
            let _sent = reply.send(operations::request(connection, &task_id));
        }
        Command::Events {
            task_id,
            after,
            reply,
        } => {
            let _sent = reply.send(operations::events_after(connection, &task_id, after));
        }
        Command::Observe {
            task_id,
            after,
            reply,
        } => {
            let _sent = reply.send(operations::observe(connection, &task_id, after));
        }
        Command::Rebuild { task_id, reply } => {
            let _sent = reply.send(operations::rebuild(connection, &task_id));
        }
        Command::SaveCheckpoint { checkpoint, reply } => {
            let _sent = reply.send(records::save_checkpoint(connection, &checkpoint));
        }
        Command::LatestCheckpoint { task_id, reply } => {
            let _sent = reply.send(records::latest_checkpoint(connection, &task_id));
        }
        Command::Nonterminal { reply } => {
            let _sent = reply.send(records::nonterminal(connection));
        }
        Command::CommitReceipt {
            receipt,
            mode,
            reply,
        } => {
            let _sent = reply.send(records::commit_receipt(connection, &receipt, &mode));
        }
        Command::Finalize {
            input,
            receipt,
            mode,
            reply,
        } => {
            let result = records::finalize(connection, *input, &receipt, &mode)
                .map(|(append, message)| FinalizeOutcome { append, message });
            let _sent = reply.send(result);
        }
        Command::Pending { consumer_id, reply } => {
            let _sent = reply.send(records::pending(connection, &consumer_id));
        }
        Command::Result { task_id, reply } => {
            let _sent = reply.send(records::result(connection, &task_id));
        }
        Command::Cancel {
            task_id,
            reason,
            reply,
        } => {
            let _sent = reply.send(records::cancel(connection, &task_id, &reason));
        }
        Command::Acknowledge {
            message_id,
            consumer_id,
            reply,
        } => {
            let _sent = reply.send(records::acknowledge(connection, &message_id, &consumer_id));
        }
        Command::Lease {
            input,
            lease_id,
            server_epoch,
            worker_id,
            expires_at,
            reply,
        } => {
            let result = dispatch::lease(
                connection,
                *input,
                &lease_id,
                &server_epoch,
                &worker_id,
                &expires_at,
            );
            let _sent = reply.send(result);
        }
        Command::RegisterProcess {
            task_id,
            lease_id,
            process_group,
            process_signature,
            started_at,
            reply,
        } => {
            let result = dispatch::register_process(
                connection,
                &task_id,
                &lease_id,
                process_group,
                &process_signature,
                &started_at,
            );
            let _sent = reply.send(result);
        }
        Command::CompleteDispatch { task_id, reply } => {
            let _sent = reply.send(dispatch::complete(connection, &task_id));
        }
        Command::RecoverDispatches { reply } => {
            let _sent = reply.send(dispatch::startup(connection));
        }
        Command::ReconcileUncertain {
            task_id,
            reason,
            reply,
        } => {
            let _sent = reply.send(records::reconcile_uncertain(connection, &task_id, &reason));
        }
        Command::DispatchState { task_id, reply } => {
            let _sent = reply.send(dispatch::state(connection, &task_id));
        }
        Command::Shutdown { reply } => {
            let result = connection
                .execute_batch("PRAGMA wal_checkpoint(TRUNCATE);")
                .map_err(Error::from);
            let _sent = reply.send(result);
            return true;
        }
    }
    false
}
