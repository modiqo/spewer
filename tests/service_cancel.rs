//! Process-level cancellation test for the local service boundary.

#![cfg(unix)]

use spewer::protocol::{TaskHandle, TaskStatus};
use spewer::store::CancelOutcome;
use std::fs::Permissions;
use std::io::{BufRead, BufReader, Read};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Output, Stdio};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

#[test]
fn cancellation_stops_app_server_and_retains_result() -> Result<(), Box<dyn std::error::Error>> {
    let root = temporary()?;
    let repository = root.join("repository");
    let home = root.join("home");
    std::fs::create_dir_all(&repository)?;
    std::fs::create_dir_all(&home)?;
    initialize_repository(&repository)?;
    let pid_path = root.join("app-server.pid");
    let fake = root.join("codex-waiting");
    std::fs::write(&fake, waiting_app_server(&pid_path))?;
    std::fs::set_permissions(&fake, Permissions::from_mode(0o700))?;
    let task = root.join("task.json");
    write_task(&task, &repository)?;

    let mut service = start_service(&home, &fake)?;
    wait_ready(&mut service)?;
    let submitted = run_cli(&home, &fake, &["submit", path(&task)?])?;
    ensure_success(&submitted, "submit cancellable task")?;
    let handle: TaskHandle = serde_json::from_slice(&submitted.stdout)?;
    let child_pid = wait_pid(&pid_path)?;

    let pending = run_cli(&home, &fake, &["result", &handle.task_id])?;
    ensure_success(&pending, "pending result")?;
    let pending: serde_json::Value = serde_json::from_slice(&pending.stdout)?;
    assert_eq!(pending.get("ready"), Some(&serde_json::Value::Bool(false)));

    let cancelled = run_cli(
        &home,
        &fake,
        &[
            "cancel",
            &handle.task_id,
            "--reason",
            "parent stopped active work",
        ],
    )?;
    ensure_success(&cancelled, "cancel")?;
    let cancelled: CancelOutcome = serde_json::from_slice(&cancelled.stdout)?;
    assert!(cancelled.changed);
    assert_eq!(cancelled.projection.status, TaskStatus::Cancelled);
    wait_process_gone(child_pid)?;

    let repeated = run_cli(&home, &fake, &["cancel", &handle.task_id])?;
    ensure_success(&repeated, "repeat cancel")?;
    let repeated: CancelOutcome = serde_json::from_slice(&repeated.stdout)?;
    assert!(!repeated.changed);

    let result = run_cli(&home, &fake, &["result", &handle.task_id])?;
    ensure_success(&result, "cancelled result")?;
    let result: serde_json::Value = serde_json::from_slice(&result.stdout)?;
    assert_eq!(result.get("ready"), Some(&serde_json::Value::Bool(true)));
    assert_eq!(
        result
            .pointer("/result/projection/status")
            .and_then(serde_json::Value::as_str),
        Some("cancelled")
    );

    let stopped = run_cli(&home, &fake, &["stop"])?;
    ensure_success(&stopped, "stop after cancel")?;
    wait_service_exit(&mut service)?;
    std::fs::remove_dir_all(root)?;
    Ok(())
}

fn waiting_app_server(pid_path: &Path) -> String {
    r#"#!/bin/sh
trap 'exit 0' TERM
printf '%s' "$$" > '__PID_PATH__'
IFS= read -r initialize
printf '%s\n' '{"id":1,"result":{"ready":true}}'
IFS= read -r initialized
IFS= read -r models
printf '%s\n' '{"id":2,"result":{"data":[{"id":"gpt-5.6-luna","model":"gpt-5.6-luna"}]}}'
IFS= read -r thread
printf '%s\n' '{"id":3,"result":{"thread":{"id":"thr_cancel","sessionId":"ses_cancel"}}}'
IFS= read -r turn
printf '%s\n' '{"id":4,"result":{"turn":{"id":"turn_cancel","status":"inProgress","items":[],"error":null}}}'
printf '%s\n' '{"method":"turn/started","params":{"threadId":"thr_cancel","turn":{"id":"turn_cancel","status":"inProgress"}}}'
while IFS= read -r line; do :; done
"#
    .replace("__PID_PATH__", &pid_path.to_string_lossy())
}

fn start_service(home: &Path, fake: &Path) -> Result<Child, Box<dyn std::error::Error>> {
    Ok(Command::new(env!("CARGO_BIN_EXE_spewer"))
        .args([
            "serve",
            "--engine",
            "codex",
            "--max-workers",
            "1",
            "--foreground",
        ])
        .env("SPEWER_HOME", home)
        .env("SPEWER_CODEX_BIN", fake)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?)
}

fn wait_ready(service: &mut Child) -> Result<(), Box<dyn std::error::Error>> {
    let stdout = service.stdout.take().ok_or("service stdout missing")?;
    let mut reader = BufReader::new(stdout);
    let mut line = String::new();
    reader.read_line(&mut line)?;
    let ready: serde_json::Value = serde_json::from_str(&line)
        .map_err(|error| format!("service readiness was not JSON: {error}; line={line:?}"))?;
    if ready.get("ready").and_then(serde_json::Value::as_bool) != Some(true) {
        return Err(format!("service did not become ready: {line}").into());
    }
    Ok(())
}

fn wait_pid(path: &Path) -> Result<u32, Box<dyn std::error::Error>> {
    for _attempt in 0..200 {
        if let Ok(value) = std::fs::read_to_string(path)
            && let Ok(pid) = value.parse::<u32>()
        {
            return Ok(pid);
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    Err("App Server did not publish its process id".into())
}

fn wait_process_gone(pid: u32) -> Result<(), Box<dyn std::error::Error>> {
    for _attempt in 0..200 {
        if !Command::new("kill")
            .args(["-0", &pid.to_string()])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()?
            .success()
        {
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    Err(format!("App Server process {pid} survived cancellation").into())
}

fn wait_service_exit(service: &mut Child) -> Result<(), Box<dyn std::error::Error>> {
    for _attempt in 0..200 {
        if let Some(status) = service.try_wait()? {
            if status.success() {
                return Ok(());
            }
            let mut stderr = String::new();
            if let Some(mut stream) = service.stderr.take() {
                stream.read_to_string(&mut stderr)?;
            }
            return Err(format!("service failed: {status}: {stderr}").into());
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    service.kill()?;
    let _status = service.wait()?;
    Err("service did not stop after drain".into())
}

fn run_cli(home: &Path, fake: &Path, arguments: &[&str]) -> std::io::Result<Output> {
    Command::new(env!("CARGO_BIN_EXE_spewer"))
        .args(arguments)
        .env("HOME", home)
        .env("SPEWER_HOME", home)
        .env("SPEWER_CODEX_BIN", fake)
        .output()
}

fn write_task(path: &Path, repository: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let task = serde_json::json!({
        "protocol_version": "0.1",
        "idempotency_key": "service-cancel",
        "objective": "Wait until cancelled without changing files.",
        "acceptance": ["Cancellation becomes terminal"],
        "workspace": {"path": repository},
        "permissions": {
            "filesystem": "workspace-write",
            "network": "deny",
            "commands": "engine-policy",
            "writable_paths": []
        },
        "budgets": {
            "wall_seconds": 30,
            "tokens": 1000,
            "tool_calls": 10,
            "retries": 0,
            "cost_usd": 1.0
        },
        "engine": {"kind": "codex-app-server"},
        "callback": {"mode": "poll", "consumer_id": "play"}
    });
    std::fs::write(path, serde_json::to_vec_pretty(&task)?)?;
    Ok(())
}

fn initialize_repository(path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    git(path, &["init", "-q"])?;
    git(path, &["config", "user.email", "spewer@example.invalid"])?;
    git(path, &["config", "user.name", "Spewer Test"])?;
    std::fs::write(path.join("README.md"), "fixture\n")?;
    git(path, &["add", "README.md"])?;
    git(path, &["commit", "-qm", "fixture"])
}

fn git(path: &Path, arguments: &[&str]) -> Result<(), Box<dyn std::error::Error>> {
    let output = Command::new("git")
        .arg("-C")
        .arg(path)
        .args(arguments)
        .output()?;
    ensure_success(&output, "git")
}

fn ensure_success(output: &Output, command: &str) -> Result<(), Box<dyn std::error::Error>> {
    if output.status.success() {
        return Ok(());
    }
    Err(format!(
        "{command} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    )
    .into())
}

fn temporary() -> Result<PathBuf, Box<dyn std::error::Error>> {
    let nanos = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
    Ok(std::env::temp_dir().join(format!("spewer-c-{}-{nanos}", std::process::id())))
}

fn path(path: &Path) -> Result<&str, Box<dyn std::error::Error>> {
    path.to_str().ok_or_else(|| "path is not UTF-8".into())
}
