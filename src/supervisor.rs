//! FIFO turn scheduling across a bounded set of App Server workers.

#[cfg(test)]
mod tests;

use crate::codex::CodexConfig;
use crate::error::{Error, ErrorKind, Result};
use crate::protocol::{TaskHandle, TaskRequest};
use crate::store::{Database, EventInput};
use crate::util::{new_id, now};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::VecDeque;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use tokio::sync::{mpsc, oneshot};
use tokio::task::{JoinHandle, JoinSet};

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
    pub fn start_codex(
        database: Database,
        codex: CodexConfig,
        config: SupervisorConfig,
    ) -> Result<Self> {
        Self::start_with(database, Arc::new(CodexWorker { config: codex }), config)
    }

    fn start_with(
        database: Database,
        worker: Arc<dyn TurnWorker>,
        config: SupervisorConfig,
    ) -> Result<Self> {
        let config = config.validate()?;
        let (commands, receiver) = mpsc::channel(COMMAND_CAPACITY);
        let manager = tokio::spawn(run_manager(database, worker, config, receiver));
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
}

enum Command {
    Submit {
        request: Box<TaskRequest>,
        reply: oneshot::Sender<Result<TaskHandle>>,
    },
    Load {
        reply: oneshot::Sender<SupervisorLoad>,
    },
    Shutdown,
}

struct Job {
    request: TaskRequest,
    task_id: String,
}

trait TurnWorker: Send + Sync {
    fn run(
        &self,
        request: TaskRequest,
        task_id: String,
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
        database: Arc<Database>,
    ) -> Pin<Box<dyn Future<Output = Result<()>> + Send>> {
        let config = self.config.clone();
        Box::pin(async move {
            let _result =
                crate::runner::run_codex_accepted(request, task_id, config, &database).await?;
            Ok(())
        })
    }
}

async fn run_manager(
    database: Database,
    worker: Arc<dyn TurnWorker>,
    config: SupervisorConfig,
    receiver: mpsc::Receiver<Command>,
) -> Result<()> {
    let database = Arc::new(database);
    let result = manager_loop(database.clone(), worker, config, receiver).await;
    let database = Arc::try_unwrap(database).map_err(|_| {
        Error::new(
            ErrorKind::Join,
            "database still has worker owners after supervisor drain",
        )
    })?;
    let close = database.close().await;
    match (result, close) {
        (Ok(()), Ok(())) => Ok(()),
        (Err(error), _) | (Ok(()), Err(error)) => Err(error),
    }
}

async fn manager_loop(
    database: Arc<Database>,
    worker: Arc<dyn TurnWorker>,
    config: SupervisorConfig,
    mut receiver: mpsc::Receiver<Command>,
) -> Result<()> {
    let mut queue = VecDeque::<Job>::new();
    let mut active = JoinSet::<Result<bool>>::new();
    let mut accepted_tasks = 0_u64;
    let mut finished_turns = 0_u64;
    let mut failed_turns = 0_u64;
    let mut draining = false;
    loop {
        dispatch(&database, &worker, config, &mut queue, &mut active).await?;
        if draining && queue.is_empty() && active.is_empty() {
            break;
        }
        if active.is_empty() {
            match receiver.recv().await {
                Some(command) => {
                    let snapshot = load(
                        config,
                        &queue,
                        &active,
                        accepted_tasks,
                        finished_turns,
                        failed_turns,
                        draining,
                    );
                    handle_command(
                        command,
                        &database,
                        &mut queue,
                        &mut accepted_tasks,
                        &mut draining,
                        snapshot,
                    )
                    .await?;
                }
                None => draining = true,
            }
            continue;
        }
        tokio::select! {
            command = receiver.recv(), if !draining => {
                match command {
                    Some(command) => {
                        let snapshot = load(
                            config,
                            &queue,
                            &active,
                            accepted_tasks,
                            finished_turns,
                            failed_turns,
                            draining,
                        );
                        handle_command(
                            command,
                            &database,
                            &mut queue,
                            &mut accepted_tasks,
                            &mut draining,
                            snapshot,
                        ).await?;
                    }
                    None => draining = true,
                }
            }
            joined = active.join_next() => {
                let failed = joined.ok_or_else(|| Error::new(ErrorKind::Join, "worker set ended early"))???;
                finished_turns = finished_turns.checked_add(1)
                    .ok_or_else(|| Error::new(ErrorKind::InvalidInput, "finished counter exhausted"))?;
                if failed {
                    failed_turns = failed_turns.checked_add(1)
                        .ok_or_else(|| Error::new(ErrorKind::InvalidInput, "failure counter exhausted"))?;
                }
            }
        }
    }
    Ok(())
}

async fn handle_command(
    command: Command,
    database: &Arc<Database>,
    queue: &mut VecDeque<Job>,
    accepted_tasks: &mut u64,
    draining: &mut bool,
    snapshot: SupervisorLoad,
) -> Result<()> {
    match command {
        Command::Submit { mut request, reply } => {
            if *draining {
                let _sent = reply.send(Err(Error::new(
                    ErrorKind::InvalidInput,
                    "supervisor is draining",
                )));
                return Ok(());
            }
            request.validate()?;
            let task_id = match &request.task_id {
                Some(task_id) => task_id.clone(),
                None => new_id("tsk")?,
            };
            request.task_id = Some(task_id.clone());
            let accepted = database
                .accept((*request).clone(), task_id.clone(), now()?)
                .await?;
            if accepted.created {
                queue.push_back(Job {
                    request: *request,
                    task_id,
                });
                *accepted_tasks = accepted_tasks.checked_add(1).ok_or_else(|| {
                    Error::new(ErrorKind::InvalidInput, "accepted counter exhausted")
                })?;
            }
            let _sent = reply.send(Ok(accepted.handle));
        }
        Command::Load { reply } => {
            let _sent = reply.send(snapshot);
        }
        Command::Shutdown => *draining = true,
    }
    Ok(())
}

async fn dispatch(
    database: &Arc<Database>,
    worker: &Arc<dyn TurnWorker>,
    config: SupervisorConfig,
    queue: &mut VecDeque<Job>,
    active: &mut JoinSet<Result<bool>>,
) -> Result<()> {
    while active.len() < config.max_workers {
        let Some(job) = queue.pop_front() else {
            break;
        };
        let lease_id = new_id("les")?;
        let worker_id = new_id("wrk")?;
        let projection = database
            .get(job.task_id.clone())
            .await?
            .ok_or_else(|| Error::new(ErrorKind::InvalidInput, "queued task does not exist"))?;
        database
            .append(EventInput {
                task_id: job.task_id.clone(),
                attempt: projection.attempt,
                kind: "turn.leased".to_owned(),
                data: json!({"lease_id": lease_id, "worker_id": worker_id}),
                source: None,
                source_key: None,
                observed_at: now()?,
            })
            .await?;
        let database = database.clone();
        let worker = worker.clone();
        active.spawn(async move {
            match worker
                .run(job.request.clone(), job.task_id.clone(), database.clone())
                .await
            {
                Ok(()) => Ok(false),
                Err(error) => {
                    crate::runner::fail_durable(&database, &job.request, job.task_id, &error)
                        .await?;
                    Ok(true)
                }
            }
        });
    }
    Ok(())
}

fn load(
    config: SupervisorConfig,
    queue: &VecDeque<Job>,
    active: &JoinSet<Result<bool>>,
    accepted_tasks: u64,
    finished_turns: u64,
    failed_turns: u64,
    draining: bool,
) -> SupervisorLoad {
    SupervisorLoad {
        queued_turns: queue.len(),
        active_turns: active.len(),
        max_workers: config.max_workers,
        accepted_tasks,
        finished_turns,
        failed_turns,
        draining,
    }
}

fn closed() -> Error {
    Error::new(ErrorKind::ChannelClosed, "supervisor manager closed")
}
