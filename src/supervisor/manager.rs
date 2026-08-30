//! Private queue ownership and worker dispatch for the supervisor.

use super::{Command, SupervisorConfig, SupervisorLoad, TurnWorker};
use crate::error::{Error, ErrorKind, Result};
use crate::protocol::{TaskInputResponse, TaskRequest};
use crate::store::{Database, EventInput};
use crate::util::{after_seconds, new_id, now};
use serde_json::json;
use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio::sync::oneshot;
use tokio::task::{AbortHandle, JoinSet};

struct Job {
    request: TaskRequest,
    task_id: String,
}

struct WorkerCompletion {
    task_id: String,
    failed: bool,
}

struct ManagerState {
    queue: VecDeque<Job>,
    active: JoinSet<Result<WorkerCompletion>>,
    active_tasks: HashMap<String, AbortHandle>,
    active_inputs: HashMap<String, mpsc::Sender<TaskInputResponse>>,
    accepted_tasks: u64,
    finished_turns: u64,
    failed_turns: u64,
    draining: bool,
}

impl ManagerState {
    fn new(queue: VecDeque<Job>) -> Self {
        Self {
            queue,
            active: JoinSet::new(),
            active_tasks: HashMap::new(),
            active_inputs: HashMap::new(),
            accepted_tasks: 0,
            finished_turns: 0,
            failed_turns: 0,
            draining: false,
        }
    }

    fn snapshot(&self, config: SupervisorConfig) -> SupervisorLoad {
        load(
            config,
            &self.queue,
            &self.active,
            self.accepted_tasks,
            self.finished_turns,
            self.failed_turns,
            self.draining,
        )
    }
}

pub(super) async fn run(
    database: Database,
    worker: Arc<dyn TurnWorker>,
    config: SupervisorConfig,
    receiver: mpsc::Receiver<Command>,
    ready: oneshot::Sender<Result<()>>,
) -> Result<()> {
    let database = Arc::new(database);
    let result = manager_loop(database.clone(), worker, config, receiver, ready).await;
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
    ready: oneshot::Sender<Result<()>>,
) -> Result<()> {
    let startup = recover_startup(&database).await;
    let queue = match startup {
        Ok(queue) => {
            let _sent = ready.send(Ok(()));
            queue
        }
        Err(error) => {
            let ready_error = Error::new(error.kind(), error.message());
            let _sent = ready.send(Err(ready_error));
            return Err(error);
        }
    };
    let server_epoch = new_id("srv")?;
    let mut state = ManagerState::new(queue);
    loop {
        dispatch(&database, &worker, config, &mut state, &server_epoch).await?;
        if state.draining && state.queue.is_empty() && state.active.is_empty() {
            break;
        }
        if state.active.is_empty() {
            match receiver.recv().await {
                Some(command) => {
                    let snapshot = state.snapshot(config);
                    handle_command(command, &database, &mut state, snapshot).await?;
                }
                None => state.draining = true,
            }
            continue;
        }
        tokio::select! {
            command = receiver.recv(), if !state.draining => {
                match command {
                    Some(command) => {
                        let snapshot = state.snapshot(config);
                        handle_command(command, &database, &mut state, snapshot).await?;
                    }
                    None => state.draining = true,
                }
            }
            joined = state.active.join_next() => {
                let joined = joined.ok_or_else(|| Error::new(ErrorKind::Join, "worker set ended early"))?;
                record_completion(joined, &mut state)?;
            }
        }
    }
    Ok(())
}

fn record_completion(
    joined: std::result::Result<Result<WorkerCompletion>, tokio::task::JoinError>,
    state: &mut ManagerState,
) -> Result<()> {
    let failed = match joined {
        Ok(Ok(completion)) => {
            state.active_tasks.remove(&completion.task_id);
            state.active_inputs.remove(&completion.task_id);
            completion.failed
        }
        Ok(Err(error)) => return Err(error),
        Err(error) if error.is_cancelled() => {
            state.active_tasks.retain(|_, handle| !handle.is_finished());
            false
        }
        Err(error) => return Err(error.into()),
    };
    state.finished_turns = state
        .finished_turns
        .checked_add(1)
        .ok_or_else(|| Error::new(ErrorKind::InvalidInput, "finished counter exhausted"))?;
    if failed {
        state.failed_turns = state
            .failed_turns
            .checked_add(1)
            .ok_or_else(|| Error::new(ErrorKind::InvalidInput, "failure counter exhausted"))?;
    }
    Ok(())
}

async fn handle_command(
    command: Command,
    database: &Arc<Database>,
    state: &mut ManagerState,
    snapshot: SupervisorLoad,
) -> Result<()> {
    match command {
        Command::Submit { mut request, reply } => {
            if state.draining {
                let _sent = reply.send(Err(Error::new(
                    ErrorKind::InvalidInput,
                    "supervisor is draining",
                )));
                return Ok(());
            }
            request.validate()?;
            crate::capsule::resolve_external_request(&mut request)?;
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
                state.queue.push_back(Job {
                    request: *request,
                    task_id,
                });
                state.accepted_tasks = state.accepted_tasks.checked_add(1).ok_or_else(|| {
                    Error::new(ErrorKind::InvalidInput, "accepted counter exhausted")
                })?;
            }
            let _sent = reply.send(Ok(accepted.handle));
        }
        Command::Load { reply } => {
            let _sent = reply.send(snapshot);
        }
        Command::Observe {
            task_id,
            after,
            reply,
        } => {
            let _sent = reply.send(database.observe(task_id, after).await);
        }
        Command::Result { task_id, reply } => {
            let _sent = reply.send(database.result(task_id).await);
        }
        Command::Cancel {
            task_id,
            reason,
            reply,
        } => {
            if let Some(position) = state.queue.iter().position(|job| job.task_id == task_id) {
                state.queue.remove(position);
            }
            if let Some(worker) = state.active_tasks.remove(&task_id) {
                state.active_inputs.remove(&task_id);
                worker.abort();
            }
            let _sent = reply.send(database.cancel(task_id, reason).await);
        }
        Command::Respond {
            task_id,
            response,
            reply,
        } => {
            let result = respond(database, &state.active_inputs, task_id, response).await;
            let _sent = reply.send(result);
        }
        Command::Acknowledge {
            message_id,
            consumer_id,
            reply,
        } => {
            let _sent = reply.send(database.acknowledge(message_id, consumer_id).await);
        }
        Command::Shutdown => state.draining = true,
    }
    Ok(())
}

async fn dispatch(
    database: &Arc<Database>,
    worker: &Arc<dyn TurnWorker>,
    config: SupervisorConfig,
    state: &mut ManagerState,
    server_epoch: &str,
) -> Result<()> {
    while state.active.len() < config.max_workers {
        let Some(job) = state.queue.pop_front() else {
            break;
        };
        let projection = database
            .get(job.task_id.clone())
            .await?
            .ok_or_else(|| Error::new(ErrorKind::InvalidInput, "queued task does not exist"))?;
        let lease_id = new_id("les")?;
        let worker_id = new_id("wrk")?;
        let lease_seconds = job.request.budgets.wall_seconds.saturating_add(60);
        database
            .lease(EventInput {
                task_id: job.task_id.clone(),
                attempt: projection.attempt,
                kind: "turn.leased".to_owned(),
                data: json!({"lease_id": lease_id, "worker_id": worker_id, "server_epoch": server_epoch}),
                source: None,
                source_key: None,
                observed_at: now()?,
            }, lease_id.clone(), server_epoch.to_owned(), worker_id, after_seconds(lease_seconds)?)
            .await?;
        let task_id = job.task_id.clone();
        let input_task_id = task_id.clone();
        let worker_task_id = task_id.clone();
        let (input_tx, input_rx) = mpsc::channel(1);
        let database = database.clone();
        let worker = worker.clone();
        let abort = state.active.spawn(async move {
            match worker
                .run(
                    job.request.clone(),
                    job.task_id.clone(),
                    lease_id,
                    database.clone(),
                    input_rx,
                )
                .await
            {
                Ok(()) => {
                    let projection = database.get(job.task_id.clone()).await?.ok_or_else(|| {
                        Error::new(ErrorKind::InvalidInput, "worker task disappeared")
                    })?;
                    let failed = if projection.status.is_terminal() {
                        false
                    } else {
                        let error = Error::new(
                            ErrorKind::EngineProtocol,
                            "worker returned without a terminal event",
                        );
                        crate::runner::fail_durable(
                            &database,
                            &job.request,
                            job.task_id.clone(),
                            &error,
                        )
                        .await?;
                        true
                    };
                    database.complete_dispatch(job.task_id).await?;
                    Ok(WorkerCompletion {
                        task_id: worker_task_id,
                        failed,
                    })
                }
                Err(error) => {
                    crate::runner::fail_durable(
                        &database,
                        &job.request,
                        job.task_id.clone(),
                        &error,
                    )
                    .await?;
                    database.complete_dispatch(job.task_id).await?;
                    Ok(WorkerCompletion {
                        task_id: worker_task_id,
                        failed: true,
                    })
                }
            }
        });
        state.active_tasks.insert(task_id, abort);
        state.active_inputs.insert(input_task_id, input_tx);
    }
    Ok(())
}

async fn respond(
    database: &Arc<Database>,
    active_inputs: &HashMap<String, mpsc::Sender<TaskInputResponse>>,
    task_id: String,
    response: TaskInputResponse,
) -> Result<crate::reducer::Projection> {
    let sender = active_inputs.get(&task_id).cloned().ok_or_else(|| {
        Error::new(
            ErrorKind::InvalidInput,
            "task has no active worker waiting for input",
        )
    })?;
    let projection = database
        .get(task_id.clone())
        .await?
        .ok_or_else(|| Error::new(ErrorKind::InvalidInput, "task does not exist"))?;
    let pending = projection
        .pending_input
        .as_ref()
        .ok_or_else(|| Error::new(ErrorKind::InvalidInput, "task is not waiting for input"))?;
    super::input::validate(pending, &response)?;
    let outcome = database
        .append(EventInput {
            task_id,
            attempt: projection.attempt,
            kind: "input.resolved".to_owned(),
            data: json!({
                "request_id": response.request_id.clone(),
                "response": response.response.clone()
            }),
            source: None,
            source_key: None,
            observed_at: now()?,
        })
        .await?;
    sender
        .send(response)
        .await
        .map_err(|_| Error::new(ErrorKind::ChannelClosed, "input worker closed"))?;
    Ok(outcome.projection)
}

async fn recover_startup(database: &Arc<Database>) -> Result<VecDeque<Job>> {
    let recovery = database.recover_dispatches().await?;
    for uncertain in recovery.uncertain {
        let process_note = super::process_custody::reap(&uncertain).await?;
        if uncertain.terminal {
            database.complete_dispatch(uncertain.task_id).await?;
        } else {
            database
                .reconcile_uncertain(
                    uncertain.task_id,
                    format!("service restarted with uncertain execution state; {process_note}"),
                )
                .await?;
        }
    }
    Ok(recovery
        .runnable
        .into_iter()
        .map(|job| Job {
            request: job.request,
            task_id: job.task_id,
        })
        .collect())
}

fn load(
    config: SupervisorConfig,
    queue: &VecDeque<Job>,
    active: &JoinSet<Result<WorkerCompletion>>,
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
