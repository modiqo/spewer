//! Single-writer SQLite storage behind bounded commands.

mod api;
mod dispatch;
mod operations;
mod records;
mod schema;
mod writer;

use crate::delivery::OutboxMessage;
use crate::error::{Error, ErrorKind, Result};
use crate::protocol::{Checkpoint, Event, EventSource, Receipt, TaskHandle, TaskRequest};
use crate::reducer::Projection;
use serde_json::Value;
use std::path::PathBuf;
use std::thread::JoinHandle;
use tokio::sync::{mpsc, oneshot};

pub use dispatch::{RecoveryBatch, RecoveryJob, UncertainDispatch};

const STORE_CAPACITY: usize = 64;

/// Result of an idempotent task acceptance transaction.
#[derive(Clone, Debug)]
pub struct AcceptedTask {
    /// Durable parent handle.
    pub handle: TaskHandle,
    /// Projection after the acceptance event.
    pub projection: Projection,
    /// Acceptance event when this call created the task.
    pub event: Option<Event>,
    /// Whether this call created durable state.
    pub created: bool,
}

/// Input for one transactional event append.
#[derive(Clone, Debug)]
pub struct EventInput {
    /// Durable task identifier.
    pub task_id: String,
    /// Attempt number.
    pub attempt: u32,
    /// Stable normalized event type.
    pub kind: String,
    /// Normalized event data.
    pub data: Value,
    /// Optional engine source metadata.
    pub source: Option<EventSource>,
    /// Optional source deduplication key.
    pub source_key: Option<String>,
    /// RFC 3339 observation time.
    pub observed_at: String,
}

/// Result of a transactional event append.
#[derive(Clone, Debug)]
pub struct AppendOutcome {
    /// Existing or newly inserted event.
    pub event: Event,
    /// Current projection after the transaction.
    pub projection: Projection,
    /// Whether the transaction inserted an event.
    pub inserted: bool,
}

/// Atomic terminal event, receipt, and callback result.
#[derive(Clone, Debug)]
pub struct FinalizeOutcome {
    /// Terminal event append outcome.
    pub append: AppendOutcome,
    /// Stable callback stored in the same transaction.
    pub message: OutboxMessage,
}

/// Atomic parent cancellation result.
#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
pub struct CancelOutcome {
    /// Projection after cancellation or the existing terminal state.
    pub projection: Projection,
    /// Stable cancellation callback when cancellation won the terminal race.
    pub message: Option<OutboxMessage>,
    /// Whether this call committed the cancellation transition.
    pub changed: bool,
}

/// One consistent projection and event replay snapshot.
#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
pub struct Observation {
    /// Current state after every event in this snapshot.
    pub projection: Projection,
    /// Committed events after the caller's cursor.
    pub events: Vec<Event>,
    /// Highest committed event sequence in the projection.
    pub next_cursor: u64,
    /// Service-recommended delay before another nonterminal observation.
    pub poll_after_ms: u64,
}

/// Non-consuming terminal-result lookup.
#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
pub struct TaskResult {
    /// Current task projection.
    pub projection: Projection,
    /// Stable terminal message when the task has produced one.
    pub message: Option<OutboxMessage>,
}

enum Command {
    Accept {
        request: Box<TaskRequest>,
        task_id: String,
        created_at: String,
        reply: oneshot::Sender<Result<AcceptedTask>>,
    },
    Append {
        input: Box<EventInput>,
        reply: oneshot::Sender<Result<AppendOutcome>>,
    },
    Get {
        task_id: String,
        reply: oneshot::Sender<Result<Option<Projection>>>,
    },
    Request {
        task_id: String,
        reply: oneshot::Sender<Result<TaskRequest>>,
    },
    Events {
        task_id: String,
        after: u64,
        reply: oneshot::Sender<Result<Vec<Event>>>,
    },
    Observe {
        task_id: String,
        after: u64,
        reply: oneshot::Sender<Result<Observation>>,
    },
    Rebuild {
        task_id: String,
        reply: oneshot::Sender<Result<Projection>>,
    },
    SaveCheckpoint {
        checkpoint: Box<Checkpoint>,
        reply: oneshot::Sender<Result<()>>,
    },
    LatestCheckpoint {
        task_id: String,
        reply: oneshot::Sender<Result<Option<Checkpoint>>>,
    },
    Nonterminal {
        reply: oneshot::Sender<Result<Vec<Projection>>>,
    },
    CommitReceipt {
        receipt: Box<Receipt>,
        mode: String,
        reply: oneshot::Sender<Result<OutboxMessage>>,
    },
    Finalize {
        input: Box<EventInput>,
        receipt: Box<Receipt>,
        mode: String,
        reply: oneshot::Sender<Result<FinalizeOutcome>>,
    },
    Pending {
        consumer_id: String,
        reply: oneshot::Sender<Result<Vec<OutboxMessage>>>,
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
    Lease {
        input: Box<EventInput>,
        lease_id: String,
        server_epoch: String,
        worker_id: String,
        expires_at: String,
        reply: oneshot::Sender<Result<AppendOutcome>>,
    },
    RegisterProcess {
        task_id: String,
        lease_id: String,
        process_group: u32,
        process_signature: String,
        started_at: String,
        reply: oneshot::Sender<Result<()>>,
    },
    CompleteDispatch {
        task_id: String,
        reply: oneshot::Sender<Result<()>>,
    },
    RecoverDispatches {
        reply: oneshot::Sender<Result<RecoveryBatch>>,
    },
    ReconcileUncertain {
        task_id: String,
        reason: String,
        reply: oneshot::Sender<Result<()>>,
    },
    DispatchState {
        task_id: String,
        reply: oneshot::Sender<Result<Option<String>>>,
    },
    Shutdown {
        reply: oneshot::Sender<Result<()>>,
    },
}

/// Exclusive handle to one tracked `SQLite` writer thread.
#[derive(Debug)]
pub struct Database {
    sender: mpsc::Sender<Command>,
    thread: Option<JoinHandle<()>>,
}

impl Database {
    /// Returns the configured durable database path.
    pub fn default_path() -> Result<PathBuf> {
        Ok(crate::util::data_root()?.join("spewer.sqlite"))
    }

    /// Opens, migrates, and starts one `SQLite` writer.
    pub async fn open(path: PathBuf) -> Result<Self> {
        create_parent(path.clone()).await?;
        let (sender, receiver) = mpsc::channel(STORE_CAPACITY);
        let (ready_tx, ready_rx) = oneshot::channel();
        let thread = std::thread::Builder::new()
            .name("spewer-sqlite-writer".to_owned())
            .spawn(move || writer::run(path, receiver, ready_tx))?;
        match ready_rx.await {
            Ok(Ok(())) => Ok(Self {
                sender,
                thread: Some(thread),
            }),
            Ok(Err(error)) => {
                join_thread(thread).await?;
                Err(error)
            }
            Err(_) => {
                join_thread(thread).await?;
                Err(Error::new(
                    ErrorKind::ChannelClosed,
                    "database writer did not initialize",
                ))
            }
        }
    }

    /// Accepts a task exactly once by idempotency key.
    pub async fn accept(
        &self,
        request: TaskRequest,
        task_id: String,
        created_at: String,
    ) -> Result<AcceptedTask> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.sender
            .send(Command::Accept {
                request: Box::new(request),
                task_id,
                created_at,
                reply: reply_tx,
            })
            .await
            .map_err(|_| closed())?;
        reply_rx.await.map_err(|_| closed())?
    }

    /// Appends and projects one event in a single transaction.
    pub async fn append(&self, input: EventInput) -> Result<AppendOutcome> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.sender
            .send(Command::Append {
                input: Box::new(input),
                reply: reply_tx,
            })
            .await
            .map_err(|_| closed())?;
        reply_rx.await.map_err(|_| closed())?
    }

    /// Loads one current task projection.
    pub async fn get(&self, task_id: String) -> Result<Option<Projection>> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.sender
            .send(Command::Get {
                task_id,
                reply: reply_tx,
            })
            .await
            .map_err(|_| closed())?;
        reply_rx.await.map_err(|_| closed())?
    }

    /// Loads committed events after a parent cursor.
    pub async fn events_after(&self, task_id: String, after: u64) -> Result<Vec<Event>> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.sender
            .send(Command::Events {
                task_id,
                after,
                reply: reply_tx,
            })
            .await
            .map_err(|_| closed())?;
        reply_rx.await.map_err(|_| closed())?
    }

    /// Returns one consistent projection and event replay snapshot.
    pub async fn observe(&self, task_id: String, after: u64) -> Result<Observation> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.sender
            .send(Command::Observe {
                task_id,
                after,
                reply: reply_tx,
            })
            .await
            .map_err(|_| closed())?;
        reply_rx.await.map_err(|_| closed())?
    }

    /// Rebuilds and stores a projection from its complete history.
    pub async fn rebuild(&self, task_id: String) -> Result<Projection> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.sender
            .send(Command::Rebuild {
                task_id,
                reply: reply_tx,
            })
            .await
            .map_err(|_| closed())?;
        reply_rx.await.map_err(|_| closed())?
    }

    /// Stops the writer and joins its operating-system thread.
    pub async fn close(mut self) -> Result<()> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.sender
            .send(Command::Shutdown { reply: reply_tx })
            .await
            .map_err(|_| closed())?;
        reply_rx.await.map_err(|_| closed())??;
        if let Some(thread) = self.thread.take() {
            join_thread(thread).await?;
        }
        Ok(())
    }
}

async fn create_parent(path: PathBuf) -> Result<()> {
    tokio::task::spawn_blocking(move || {
        if path.as_os_str() == ":memory:" {
            return Ok(());
        }
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        Ok::<(), std::io::Error>(())
    })
    .await??;
    Ok(())
}

async fn join_thread(thread: JoinHandle<()>) -> Result<()> {
    tokio::task::spawn_blocking(move || match thread.join() {
        Ok(()) => Ok(()),
        Err(_) => Err(Error::new(ErrorKind::Join, "database writer panicked")),
    })
    .await??;
    Ok(())
}

fn closed() -> Error {
    Error::new(ErrorKind::ChannelClosed, "database writer closed")
}
