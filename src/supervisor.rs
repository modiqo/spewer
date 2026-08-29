//! FIFO turn scheduling across a bounded set of App Server workers.

mod manager;
mod process_custody;
#[cfg(test)]
mod tests;

use crate::codex::CodexConfig;
use crate::error::{Error, ErrorKind, Result};
use crate::protocol::{TaskHandle, TaskRequest};
use crate::store::{CancelOutcome, Database, Observation, TaskResult};
use serde::{Deserialize, Serialize};
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use tokio::sync::{mpsc, oneshot};
use tokio::task::JoinHandle;

const COMMAND_CAPACITY: usize = 64;

/// Hard capacity settings for one local supervisor.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct SupervisorConfig {
    /// Maximum simultaneous App Server child processes.
    pub max_workers: usize,
}

impl Default for SupervisorConfig {
    fn default() -> Self {
        Self { max_workers: 1 }
    }
}

impl SupervisorConfig {
    fn validate(self) -> Result<Self> {
        if self.max_workers == 0 || self.max_workers > 64 {
            return Err(Error::new(
                ErrorKind::InvalidInput,
                "max_workers must be between 1 and 64",
            ));
        }
        Ok(self)
    }
}

/// Observable load without provider-specific worker state.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct SupervisorLoad {
    /// Tasks accepted but not yet leased.
    pub queued_turns: usize,
    /// Turns currently assigned to workers.
    pub active_turns: usize,
    /// Configured active-turn limit.
    pub max_workers: usize,
    /// New tasks accepted since service startup.
    pub accepted_tasks: u64,
    /// Workers that reached terminal state since startup.
    pub finished_turns: u64,
    /// Finished workers that required failure finalization.
    pub failed_turns: u64,
    /// Whether shutdown has stopped new submissions.
    pub draining: bool,
}

/// Ownership handle for one supervisor manager task.
#[derive(Debug)]
pub struct Supervisor {
    handle: SupervisorHandle,
    manager: JoinHandle<Result<()>>,
}

/// Cloneable command handle used by local control connections.
#[derive(Clone, Debug)]
pub struct SupervisorHandle {
    commands: mpsc::Sender<Command>,
}

impl Supervisor {
    /// Starts a supervisor that creates one Codex App Server child per leased turn.
    pub async fn start_codex(
        database: Database,
        codex: CodexConfig,
        config: SupervisorConfig,
    ) -> Result<Self> {
        Self::start_with(database, Arc::new(CodexWorker { config: codex }), config).await
    }

    async fn start_with(
        database: Database,
        worker: Arc<dyn TurnWorker>,
        config: SupervisorConfig,
    ) -> Result<Self> {
        let config = config.validate()?;
        let (commands, receiver) = mpsc::channel(COMMAND_CAPACITY);
        let (ready_tx, ready_rx) = oneshot::channel();
        let manager = tokio::spawn(manager::run(database, worker, config, receiver, ready_tx));
        ready_rx.await.map_err(|_| closed())??;
        Ok(Self {
            handle: SupervisorHandle { commands },
            manager,
        })
    }

    /// Returns a cloneable submission and observation handle.
    pub fn handle(&self) -> SupervisorHandle {
        self.handle.clone()
    }

    /// Stops acceptance, drains queued turns, and closes durable storage.
    pub async fn shutdown(self) -> Result<()> {
        self.handle
            .commands
            .send(Command::Shutdown)
            .await
            .map_err(|_| closed())?;
        self.manager.await?
    }
}

impl SupervisorHandle {
    /// Commits a task and queues its first turn without waiting for execution.
    pub async fn submit(&self, request: TaskRequest) -> Result<TaskHandle> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.commands
            .send(Command::Submit {
                request: Box::new(request),
                reply: reply_tx,
            })
            .await
            .map_err(|_| closed())?;
        reply_rx.await.map_err(|_| closed())?
    }

    /// Returns a point-in-time scheduler load report.
    pub async fn load(&self) -> Result<SupervisorLoad> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.commands
            .send(Command::Load { reply: reply_tx })
            .await
            .map_err(|_| closed())?;
        reply_rx.await.map_err(|_| closed())
    }

    /// Returns current state and committed events after a parent cursor.
    pub async fn observe(&self, task_id: String, after: u64) -> Result<Observation> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.commands
            .send(Command::Observe {
                task_id,
                after,
                reply: reply_tx,
            })
            .await
            .map_err(|_| closed())?;
        reply_rx.await.map_err(|_| closed())?
    }

    /// Returns current state and a stable terminal message when ready.
    pub async fn result(&self, task_id: String) -> Result<TaskResult> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.commands
            .send(Command::Result {
                task_id,
                reply: reply_tx,
            })
            .await
            .map_err(|_| closed())?;
        reply_rx.await.map_err(|_| closed())?
    }

    /// Cancels queued or active work and returns its terminal state.
    pub async fn cancel(&self, task_id: String, reason: String) -> Result<CancelOutcome> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.commands
            .send(Command::Cancel {
                task_id,
                reason,
                reply: reply_tx,
            })
            .await
            .map_err(|_| closed())?;
        reply_rx.await.map_err(|_| closed())?
    }

    /// Records one consumer acknowledgement through the service owner.
    pub async fn acknowledge(&self, message_id: String, consumer_id: String) -> Result<bool> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.commands
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

enum Command {
    Submit {
        request: Box<TaskRequest>,
        reply: oneshot::Sender<Result<TaskHandle>>,
    },
    Load {
        reply: oneshot::Sender<SupervisorLoad>,
    },
    Observe {
        task_id: String,
        after: u64,
        reply: oneshot::Sender<Result<Observation>>,
    },
    Result {
        task_id: String,
        reply: oneshot::Sender<Result<TaskResult>>,
    },
    Cancel {
        task_id: String,
        reason: String,
        reply: oneshot::Sender<Result<CancelOutcome>>,
    },
    Acknowledge {
        message_id: String,
        consumer_id: String,
        reply: oneshot::Sender<Result<bool>>,
    },
    Shutdown,
}

trait TurnWorker: Send + Sync {
    fn run(
        &self,
        request: TaskRequest,
        task_id: String,
        lease_id: String,
        database: Arc<Database>,
    ) -> Pin<Box<dyn Future<Output = Result<()>> + Send>>;
}

#[derive(Clone, Debug)]
struct CodexWorker {
    config: CodexConfig,
}

impl TurnWorker for CodexWorker {
    fn run(
        &self,
        request: TaskRequest,
        task_id: String,
        lease_id: String,
        database: Arc<Database>,
    ) -> Pin<Box<dyn Future<Output = Result<()>> + Send>> {
        let config = self.config.clone();
        Box::pin(async move {
            let _result =
                crate::runner::run_codex_accepted(request, task_id, lease_id, config, &database)
                    .await?;
            Ok(())
        })
    }
}

fn closed() -> Error {
    Error::new(ErrorKind::ChannelClosed, "supervisor manager closed")
}
