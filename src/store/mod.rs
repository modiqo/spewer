//! Single-writer SQLite storage behind bounded commands.

mod operations;
mod schema;

use crate::error::{Error, ErrorKind, Result};
use crate::protocol::{Event, EventSource, TaskHandle, TaskRequest};
use crate::reducer::Projection;
use rusqlite::Connection;
use serde_json::Value;
use std::path::PathBuf;
use std::thread::JoinHandle;
use tokio::sync::{mpsc, oneshot};

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
    Events {
        task_id: String,
        after: u64,
        reply: oneshot::Sender<Result<Vec<Event>>>,
    },
    Rebuild {
        task_id: String,
        reply: oneshot::Sender<Result<Projection>>,
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
            .spawn(move || writer_thread(path, receiver, ready_tx))?;
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

fn writer_thread(
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
        match command {
            Command::Accept {
                request,
                task_id,
                created_at,
                reply,
            } => {
                let _sent = reply.send(operations::accept(
                    &mut connection,
                    &request,
                    &task_id,
                    &created_at,
                ));
            }
            Command::Append { input, reply } => {
                let _sent = reply.send(operations::append(&mut connection, *input));
            }
            Command::Get { task_id, reply } => {
                let _sent = reply.send(operations::get(&connection, &task_id));
            }
            Command::Events {
                task_id,
                after,
                reply,
            } => {
                let _sent = reply.send(operations::events_after(&connection, &task_id, after));
            }
            Command::Rebuild { task_id, reply } => {
                let _sent = reply.send(operations::rebuild(&mut connection, &task_id));
            }
            Command::Shutdown { reply } => {
                let result = connection
                    .execute_batch("PRAGMA wal_checkpoint(TRUNCATE);")
                    .map_err(Error::from);
                let _sent = reply.send(result);
                break;
            }
        }
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
