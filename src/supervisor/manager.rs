//! Private queue ownership and worker dispatch for the supervisor.

use super::{Command, SupervisorConfig, SupervisorLoad, TurnWorker};
use crate::error::{Error, ErrorKind, Result};
use crate::protocol::TaskRequest;
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
    let mut queue = match startup {
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
    let mut active = JoinSet::<Result<WorkerCompletion>>::new();
    let mut active_tasks = HashMap::<String, AbortHandle>::new();
    let mut accepted_tasks = 0_u64;
    let mut finished_turns = 0_u64;
    let mut failed_turns = 0_u64;
    let mut draining = false;
    loop {
        dispatch(
            &database,
            &worker,
            config,
            &mut queue,
            &mut active,
            &mut active_tasks,
            &server_epoch,
        )
        .await?;
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
                        &mut active_tasks,
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
                            &mut active_tasks,
                            &mut accepted_tasks,
                            &mut draining,
                            snapshot,
                        ).await?;
                    }
                    None => draining = true,
                }
            }
            joined = active.join_next() => {
                let joined = joined.ok_or_else(|| Error::new(ErrorKind::Join, "worker set ended early"))?;
                record_completion(
                    joined,
                    &mut active_tasks,
                    &mut finished_turns,
                    &mut failed_turns,
                )?;
            }
        }
    }
    Ok(())
}

fn record_completion(
    joined: std::result::Result<Result<WorkerCompletion>, tokio::task::JoinError>,
    active_tasks: &mut HashMap<String, AbortHandle>,
    finished_turns: &mut u64,
    failed_turns: &mut u64,
) -> Result<()> {
    let failed = match joined {
        Ok(Ok(completion)) => {
            active_tasks.remove(&completion.task_id);
            completion.failed
        }
        Ok(Err(error)) => return Err(error),
        Err(error) if error.is_cancelled() => {
            active_tasks.retain(|_, handle| !handle.is_finished());
            false
        }
        Err(error) => return Err(error.into()),
    };
    *finished_turns = finished_turns
        .checked_add(1)
        .ok_or_else(|| Error::new(ErrorKind::InvalidInput, "finished counter exhausted"))?;
    if failed {
        *failed_turns = failed_turns
            .checked_add(1)
            .ok_or_else(|| Error::new(ErrorKind::InvalidInput, "failure counter exhausted"))?;
    }
    Ok(())
}

async fn handle_command(
    command: Command,
    database: &Arc<Database>,
    queue: &mut VecDeque<Job>,
    active_tasks: &mut HashMap<String, AbortHandle>,
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
            if let Some(position) = queue.iter().position(|job| job.task_id == task_id) {
                queue.remove(position);
            }
            if let Some(worker) = active_tasks.remove(&task_id) {
                worker.abort();
            }
            let _sent = reply.send(database.cancel(task_id, reason).await);
        }
        Command::Acknowledge {
            message_id,
            consumer_id,
            reply,
        } => {
            let _sent = reply.send(database.acknowledge(message_id, consumer_id).await);
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
    active: &mut JoinSet<Result<WorkerCompletion>>,
    active_tasks: &mut HashMap<String, AbortHandle>,
    server_epoch: &str,
) -> Result<()> {
    while active.len() < config.max_workers {
        let Some(job) = queue.pop_front() else {
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
        let worker_task_id = task_id.clone();
        let database = database.clone();
        let worker = worker.clone();
        let abort = active.spawn(async move {
            match worker
                .run(
                    job.request.clone(),
                    job.task_id.clone(),
                    lease_id,
                    database.clone(),
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
        active_tasks.insert(task_id, abort);
    }
    Ok(())
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
