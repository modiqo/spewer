use super::{Supervisor, SupervisorConfig, TurnWorker};
use crate::error::{Error, ErrorKind, Result};
use crate::protocol::TaskRequest;
use crate::store::Database;
use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

#[derive(Debug)]
struct TestWorker {
    delay: Duration,
    fail: bool,
}

impl TestWorker {
    fn new(delay: Duration, fail: bool) -> Self {
        Self { delay, fail }
    }
}

impl TurnWorker for TestWorker {
    fn run(
        &self,
        _request: TaskRequest,
        _task_id: String,
        _lease_id: String,
        _database: Arc<Database>,
    ) -> Pin<Box<dyn Future<Output = Result<()>> + Send>> {
        let delay = self.delay;
        let fail = self.fail;
        Box::pin(async move {
            tokio::time::sleep(delay).await;
            if fail {
                return Err(Error::new(ErrorKind::EngineProtocol, "injected failure"));
            }
            Ok(())
        })
    }
}

#[tokio::test(flavor = "current_thread")]
async fn capacity_one_keeps_the_second_turn_queued() -> Result<()> {
    let path = temporary("capacity")?.with_extension("sqlite");
    let database = Database::open(path.clone()).await?;
    let worker = Arc::new(TestWorker::new(Duration::from_millis(150), false));
    let supervisor =
        Supervisor::start_with(database, worker, SupervisorConfig { max_workers: 1 }).await?;
    let handle = supervisor.handle();
    let _first = handle.submit(request("capacity-one")?).await?;
    let _second = handle.submit(request("capacity-two")?).await?;
    let load = wait_for_load(&handle, 1, 1).await?;
    assert_eq!(load.max_workers, 1);
    wait_for_finished(&handle, 2).await?;
    supervisor.shutdown().await?;
    remove_database_files(&path)?;
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn worker_failure_commits_terminal_callback() -> Result<()> {
    let path = temporary("failure")?.with_extension("sqlite");
    let database = Database::open(path.clone()).await?;
    let worker = Arc::new(TestWorker::new(Duration::from_millis(1), true));
    let supervisor =
        Supervisor::start_with(database, worker, SupervisorConfig { max_workers: 1 }).await?;
    let handle = supervisor.handle();
    let task = handle.submit(request("failure-one")?).await?;
    wait_for_finished(&handle, 1).await?;
    supervisor.shutdown().await?;

    let database = Database::open(path.clone()).await?;
    let projection = database
        .get(task.task_id.clone())
        .await?
        .ok_or_else(|| Error::new(ErrorKind::InvalidInput, "missing failed task"))?;
    let pending = database.pending("play".to_owned()).await?;
    database.close().await?;
    assert_eq!(projection.status, crate::protocol::TaskStatus::Failed);
    assert_eq!(pending.len(), 1);
    remove_database_files(&path)?;
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn startup_schedules_a_task_accepted_before_the_manager_existed() -> Result<()> {
    let path = temporary("startup-queue")?.with_extension("sqlite");
    let database = Database::open(path.clone()).await?;
    let accepted = database
        .accept(
            request("startup-queue")?,
            "startup-task".to_owned(),
            "2026-08-29T00:00:00Z".to_owned(),
        )
        .await?;
    let worker = Arc::new(TestWorker::new(Duration::from_millis(1), true));
    let supervisor =
        Supervisor::start_with(database, worker, SupervisorConfig { max_workers: 1 }).await?;
    let handle = supervisor.handle();
    wait_for_finished(&handle, 1).await?;
    let result = handle.result(accepted.handle.task_id).await?;
    assert!(result.projection.status.is_terminal());
    supervisor.shutdown().await?;
    remove_database_files(&path)?;
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn duplicate_submission_does_not_queue_a_second_turn() -> Result<()> {
    let path = temporary("duplicate")?.with_extension("sqlite");
    let database = Database::open(path.clone()).await?;
    let worker = Arc::new(TestWorker::new(Duration::from_millis(50), false));
    let supervisor =
        Supervisor::start_with(database, worker, SupervisorConfig { max_workers: 1 }).await?;
    let handle = supervisor.handle();
    let first = handle.submit(request("same-key")?).await?;
    let duplicate = handle.submit(request("same-key")?).await?;
    assert_eq!(first.task_id, duplicate.task_id);
    wait_for_finished(&handle, 1).await?;
    let load = handle.load().await?;
    assert_eq!(load.accepted_tasks, 1);
    supervisor.shutdown().await?;
    remove_database_files(&path)?;
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn queued_cancellation_removes_work_and_commits_one_result() -> Result<()> {
    let path = temporary("queued-cancel")?.with_extension("sqlite");
    let database = Database::open(path.clone()).await?;
    let worker = Arc::new(TestWorker::new(Duration::from_millis(100), false));
    let supervisor =
        Supervisor::start_with(database, worker, SupervisorConfig { max_workers: 1 }).await?;
    let handle = supervisor.handle();
    let _active = handle.submit(request("cancel-active")?).await?;
    let queued = handle.submit(request("cancel-queued")?).await?;
    wait_for_load(&handle, 1, 1).await?;

    let cancelled = handle
        .cancel(
            queued.task_id.clone(),
            "parent stopped queued work".to_owned(),
        )
        .await?;
    let repeated = handle
        .cancel(queued.task_id.clone(), "duplicate cancellation".to_owned())
        .await?;
    let result = handle.result(queued.task_id).await?;

    assert!(cancelled.changed);
    assert!(!repeated.changed);
    assert_eq!(
        result.projection.status,
        crate::protocol::TaskStatus::Cancelled
    );
    assert!(result.message.is_some());
    wait_for_finished(&handle, 1).await?;
    supervisor.shutdown().await?;
    remove_database_files(&path)?;
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn running_cancellation_aborts_worker_and_is_idempotent() -> Result<()> {
    let path = temporary("running-cancel")?.with_extension("sqlite");
    let database = Database::open(path.clone()).await?;
    let worker = Arc::new(TestWorker::new(Duration::from_secs(30), false));
    let supervisor =
        Supervisor::start_with(database, worker, SupervisorConfig { max_workers: 1 }).await?;
    let handle = supervisor.handle();
    let task = handle.submit(request("cancel-running")?).await?;
    wait_for_load(&handle, 1, 0).await?;

    let first = handle
        .cancel(
            task.task_id.clone(),
            "parent stopped active work".to_owned(),
        )
        .await?;
    let first_result = handle.result(task.task_id.clone()).await?;
    let second = handle
        .cancel(task.task_id.clone(), "duplicate cancellation".to_owned())
        .await?;
    let second_result = handle.result(task.task_id).await?;

    assert!(first.changed);
    assert!(!second.changed);
    assert_eq!(first_result.message, second_result.message);
    assert_eq!(
        second_result.projection.status,
        crate::protocol::TaskStatus::Cancelled
    );
    wait_for_finished(&handle, 1).await?;
    supervisor.shutdown().await?;
    remove_database_files(&path)?;
    Ok(())
}

async fn wait_for_load(
    handle: &super::SupervisorHandle,
    active: usize,
    queued: usize,
) -> Result<super::SupervisorLoad> {
    for _attempt in 0..100 {
        let load = handle.load().await?;
        if load.active_turns == active && load.queued_turns == queued {
            return Ok(load);
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    Err(Error::new(
        ErrorKind::Timeout,
        "scheduler load did not settle",
    ))
}

async fn wait_for_finished(handle: &super::SupervisorHandle, count: u64) -> Result<()> {
    for _attempt in 0..200 {
        if handle.load().await?.finished_turns == count {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    Err(Error::new(ErrorKind::Timeout, "workers did not finish"))
}

fn request(key: &str) -> Result<TaskRequest> {
    let mut request: TaskRequest =
        serde_json::from_str(include_str!("../../tests/fixtures/task-request.json"))?;
    request.idempotency_key = key.to_owned();
    request.task_id = None;
    Ok(request)
}

fn temporary(name: &str) -> Result<PathBuf> {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| Error::new(ErrorKind::Io, error.to_string()))?
        .as_nanos();
    Ok(std::env::temp_dir().join(format!(
        "spewer-supervisor-{name}-{}-{nanos}",
        std::process::id()
    )))
}

fn remove_database_files(path: &Path) -> Result<()> {
    for candidate in [
        path.to_path_buf(),
        PathBuf::from(format!("{}-shm", path.display())),
        PathBuf::from(format!("{}-wal", path.display())),
    ] {
        match std::fs::remove_file(candidate) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
    }
    Ok(())
}
