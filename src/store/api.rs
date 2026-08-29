use super::{
    AppendOutcome, CancelOutcome, Command, Database, EventInput, FinalizeOutcome, RecoveryBatch,
    TaskResult, closed,
};
use crate::delivery::OutboxMessage;
use crate::error::Result;
use crate::protocol::{Checkpoint, Receipt, TaskRequest};
use crate::reducer::Projection;
use tokio::sync::oneshot;

impl Database {
    /// Atomically records a worker lease and its normalized event.
    pub async fn lease(
        &self,
        input: EventInput,
        lease_id: String,
        server_epoch: String,
        worker_id: String,
        expires_at: String,
    ) -> Result<AppendOutcome> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.sender
            .send(Command::Lease {
                input: Box::new(input),
                lease_id,
                server_epoch,
                worker_id,
                expires_at,
                reply: reply_tx,
            })
            .await
            .map_err(|_| closed())?;
        reply_rx.await.map_err(|_| closed())?
    }

    /// Binds an App Server process group to its durable lease before initialization.
    pub async fn register_process(
        &self,
        task_id: String,
        lease_id: String,
        process_group: u32,
        process_signature: String,
        started_at: String,
    ) -> Result<()> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.sender
            .send(Command::RegisterProcess {
                task_id,
                lease_id,
                process_group,
                process_signature,
                started_at,
                reply: reply_tx,
            })
            .await
            .map_err(|_| closed())?;
        reply_rx.await.map_err(|_| closed())?
    }

    /// Clears process custody after a task reaches immutable terminal state.
    pub async fn complete_dispatch(&self, task_id: String) -> Result<()> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.sender
            .send(Command::CompleteDispatch {
                task_id,
                reply: reply_tx,
            })
            .await
            .map_err(|_| closed())?;
        reply_rx.await.map_err(|_| closed())?
    }

    /// Reconstructs runnable and uncertain work before the service reports ready.
    pub async fn recover_dispatches(&self) -> Result<RecoveryBatch> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.sender
            .send(Command::RecoverDispatches { reply: reply_tx })
            .await
            .map_err(|_| closed())?;
        reply_rx.await.map_err(|_| closed())?
    }

    /// Converts an uncertain previous execution into one durable escalation receipt.
    pub async fn reconcile_uncertain(&self, task_id: String, reason: String) -> Result<()> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.sender
            .send(Command::ReconcileUncertain {
                task_id,
                reason,
                reply: reply_tx,
            })
            .await
            .map_err(|_| closed())?;
        reply_rx.await.map_err(|_| closed())?
    }

    /// Returns the durable scheduler state for diagnostics and contract tests.
    pub async fn dispatch_state(&self, task_id: String) -> Result<Option<String>> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.sender
            .send(Command::DispatchState {
                task_id,
                reply: reply_tx,
            })
            .await
            .map_err(|_| closed())?;
        reply_rx.await.map_err(|_| closed())?
    }

    /// Loads the immutable accepted request for recovery.
    pub async fn request(&self, task_id: String) -> Result<TaskRequest> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.sender
            .send(Command::Request {
                task_id,
                reply: reply_tx,
            })
            .await
            .map_err(|_| closed())?;
        reply_rx.await.map_err(|_| closed())?
    }

    /// Stores one named recovery boundary idempotently.
    pub async fn save_checkpoint(&self, checkpoint: Checkpoint) -> Result<()> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.sender
            .send(Command::SaveCheckpoint {
                checkpoint: Box::new(checkpoint),
                reply: reply_tx,
            })
            .await
            .map_err(|_| closed())?;
        reply_rx.await.map_err(|_| closed())?
    }

    /// Loads the latest checkpoint for one task.
    pub async fn latest_checkpoint(&self, task_id: String) -> Result<Option<Checkpoint>> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.sender
            .send(Command::LatestCheckpoint {
                task_id,
                reply: reply_tx,
            })
            .await
            .map_err(|_| closed())?;
        reply_rx.await.map_err(|_| closed())?
    }

    /// Returns all tasks that need recovery reconciliation.
    pub async fn nonterminal(&self) -> Result<Vec<Projection>> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.sender
            .send(Command::Nonterminal { reply: reply_tx })
            .await
            .map_err(|_| closed())?;
        reply_rx.await.map_err(|_| closed())?
    }

    /// Atomically stores a receipt and stable callback message.
    pub async fn commit_receipt(&self, receipt: Receipt, mode: String) -> Result<OutboxMessage> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.sender
            .send(Command::CommitReceipt {
                receipt: Box::new(receipt),
                mode,
                reply: reply_tx,
            })
            .await
            .map_err(|_| closed())?;
        reply_rx.await.map_err(|_| closed())?
    }

    /// Commits terminal event, receipt, and callback atomically.
    pub async fn finalize(
        &self,
        input: EventInput,
        receipt: Receipt,
        mode: String,
    ) -> Result<FinalizeOutcome> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.sender
            .send(Command::Finalize {
                input: Box::new(input),
                receipt: Box::new(receipt),
                mode,
                reply: reply_tx,
            })
            .await
            .map_err(|_| closed())?;
        reply_rx.await.map_err(|_| closed())?
    }

    /// Returns messages not acknowledged by a consumer.
    pub async fn pending(&self, consumer_id: String) -> Result<Vec<OutboxMessage>> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.sender
            .send(Command::Pending {
                consumer_id,
                reply: reply_tx,
            })
            .await
            .map_err(|_| closed())?;
        reply_rx.await.map_err(|_| closed())?
    }

    /// Returns the stable terminal message for one task without consuming it.
    pub async fn result(&self, task_id: String) -> Result<TaskResult> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.sender
            .send(Command::Result {
                task_id,
                reply: reply_tx,
            })
            .await
            .map_err(|_| closed())?;
        reply_rx.await.map_err(|_| closed())?
    }

    /// Atomically cancels one nonterminal task and creates its callback.
    pub async fn cancel(&self, task_id: String, reason: String) -> Result<CancelOutcome> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.sender
            .send(Command::Cancel {
                task_id,
                reason,
                reply: reply_tx,
            })
            .await
            .map_err(|_| closed())?;
        reply_rx.await.map_err(|_| closed())?
    }

    /// Records one parent acknowledgement idempotently.
    pub async fn acknowledge(&self, message_id: String, consumer_id: String) -> Result<bool> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.sender
            .send(Command::Acknowledge {
                message_id,
                consumer_id,
                reply: reply_tx,
            })
            .await
            .map_err(|_| closed())?;
        reply_rx.await.map_err(|_| closed())?
    }
}
