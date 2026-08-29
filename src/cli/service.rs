//! Foreground and detached lifecycle control for the local service.

use crate::codex::CodexConfig;
use crate::error::{Error, ErrorKind, Result};
use crate::store::Database;
use crate::supervisor::{SupervisorConfig, SupervisorLoad};
use serde::Serialize;
use serde_json::{Value, json};
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::Duration;

const STARTUP_ATTEMPTS: u16 = 250;
const STARTUP_POLL: Duration = Duration::from_millis(20);

pub(super) async fn serve(max_workers: usize, socket: Option<PathBuf>, detach: bool) -> Result<()> {
    let path = super::socket_path(socket)?;
    if detach {
        let report = ensure_detached(max_workers, path).await?;
        println!("{}", serde_json::to_string_pretty(&report)?);
        return Ok(());
    }
    run_foreground(max_workers, path).await
}

async fn run_foreground(max_workers: usize, path: PathBuf) -> Result<()> {
    detach_session_if_requested()?;
    let _capsule = crate::capsule::ensure_default()?;
    let database = Database::open(Database::default_path()?).await?;
    let service = crate::control::LocalService::bind(
        path,
        database,
        CodexConfig::default(),
        SupervisorConfig { max_workers },
    )
    .await?;
    println!(
        "{}",
        serde_json::to_string(&json!({
            "ready": true,
            "mode": "foreground",
            "socket": service.socket_path(),
            "max_workers": max_workers
        }))?
    );
    std::io::stdout().flush()?;
    service.run().await
}

#[derive(Debug, Serialize)]
pub(super) struct DetachedReport {
    ready: bool,
    mode: &'static str,
    started: bool,
    pid: Option<u32>,
    socket: PathBuf,
    log: Option<PathBuf>,
    load: SupervisorLoad,
    next: Value,
}

pub(super) async fn ensure_detached(max_workers: usize, path: PathBuf) -> Result<DetachedReport> {
    if let Ok(load) = crate::control::load(path.clone()).await {
        return Ok(detached_report(false, None, path, None, load));
    }

    let log_path = crate::util::data_root()?.join("spewer-service.log");
    let prepared_path = log_path.clone();
    let (stdout, stderr) = tokio::task::spawn_blocking(move || open_log(&prepared_path)).await??;
    let mut child = spawn_service(max_workers, &path, stdout, stderr)?;
    let pid = child.id();

    for _attempt in 0..STARTUP_ATTEMPTS {
        ensure_running(&mut child, &log_path)?;
        if let Ok(load) = crate::control::load(path.clone()).await {
            return Ok(detached_report(true, Some(pid), path, Some(log_path), load));
        }
        tokio::time::sleep(STARTUP_POLL).await;
    }

    stop_failed_child(&mut child);
    Err(Error::new(
        ErrorKind::Timeout,
        format!(
            "detached service did not become ready; inspect {}",
            log_path.display()
        ),
    ))
}

fn spawn_service(max_workers: usize, path: &Path, stdout: File, stderr: File) -> Result<Child> {
    let executable = std::env::current_exe()?;
    Command::new(executable)
        .arg("serve")
        .args(["--engine", "codex", "--max-workers"])
        .arg(max_workers.to_string())
        .arg("--socket")
        .arg(path)
        .arg("--foreground")
        .env("SPEWER_DETACHED_CHILD", "1")
        .stdin(Stdio::null())
        .stdout(Stdio::from(stdout))
        .stderr(Stdio::from(stderr))
        .spawn()
        .map_err(Into::into)
}

#[cfg(unix)]
fn detach_session_if_requested() -> Result<()> {
    if std::env::var_os("SPEWER_DETACHED_CHILD").is_none() {
        return Ok(());
    }
    nix::unistd::setsid()
        .map(|_session| ())
        .map_err(|error| Error::new(ErrorKind::Io, format!("cannot detach service: {error}")))
}

#[cfg(not(unix))]
fn detach_session_if_requested() -> Result<()> {
    if std::env::var_os("SPEWER_DETACHED_CHILD").is_some() {
        return Err(Error::new(
            ErrorKind::InvalidInput,
            "detached service requires Unix process sessions",
        ));
    }
    Ok(())
}

fn ensure_running(child: &mut Child, log_path: &Path) -> Result<()> {
    let Some(status) = child.try_wait()? else {
        return Ok(());
    };
    Err(Error::new(
        ErrorKind::Io,
        format!(
            "detached service exited with {status}; inspect {}",
            log_path.display()
        ),
    ))
}

fn stop_failed_child(child: &mut Child) {
    let _killed = child.kill();
    let _status = child.wait();
}

fn open_log(path: &Path) -> Result<(File, File)> {
    let parent = path
        .parent()
        .ok_or_else(|| Error::new(ErrorKind::InvalidInput, "service log parent is missing"))?;
    std::fs::create_dir_all(parent)?;
    set_private_directory(parent)?;
    if let Ok(metadata) = std::fs::symlink_metadata(path)
        && !metadata.file_type().is_file()
    {
        return Err(Error::new(
            ErrorKind::InvalidInput,
            "service log path is not a regular file",
        ));
    }
    let log = OpenOptions::new().create(true).append(true).open(path)?;
    set_private_file(path)?;
    let stderr = log.try_clone()?;
    Ok((log, stderr))
}

#[cfg(unix)]
fn set_private_directory(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700))?;
    Ok(())
}

#[cfg(not(unix))]
fn set_private_directory(_path: &Path) -> Result<()> {
    Ok(())
}

#[cfg(unix)]
fn set_private_file(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
    Ok(())
}

#[cfg(not(unix))]
fn set_private_file(_path: &Path) -> Result<()> {
    Ok(())
}

fn detached_report(
    started: bool,
    pid: Option<u32>,
    socket: PathBuf,
    log: Option<PathBuf>,
    load: SupervisorLoad,
) -> DetachedReport {
    let socket_arg = socket.as_os_str().to_string_lossy().into_owned();
    DetachedReport {
        ready: true,
        mode: "detached",
        started,
        pid,
        socket,
        log,
        load,
        next: json!({
            "ask": ["spewer", "ask", "<question>", "--detach", "--socket", socket_arg],
            "load": ["spewer", "load", "--socket", socket_arg],
            "stop": ["spewer", "stop", "--socket", socket_arg]
        }),
    }
}
