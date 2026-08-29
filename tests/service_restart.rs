//! Hard-crash service recovery with durable App Server process custody.

#![cfg(unix)]

use spewer::protocol::{TaskHandle, TaskStatus};
use spewer::reducer::Projection;
use std::fs::Permissions;
use std::io::{BufRead, BufReader, Read};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Output, Stdio};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const BLOCKING_APP_SERVER: &str = r#"#!/bin/sh
trap '' HUP
printf '%s\n' "$$" > "$SPEWER_TEST_PID_FILE"
IFS= read -r initialize
printf '%s\n' '{"id":1,"result":{"ready":true}}'
IFS= read -r initialized
IFS= read -r models
printf '%s\n' '{"id":2,"result":{"data":[{"id":"gpt-5.6-luna","model":"gpt-5.6-luna"}]}}'
IFS= read -r thread
printf '%s\n' '{"id":3,"result":{"thread":{"id":"thr_restart","sessionId":"ses_restart"}}}'
IFS= read -r turn
printf '%s\n' '{"id":4,"result":{"turn":{"id":"turn_restart","status":"inProgress","items":[],"error":null}}}'
printf '%s\n' '{"method":"thread/started","params":{"thread":{"id":"thr_restart","sessionId":"ses_restart"}}}'
printf '%s\n' '{"method":"turn/started","params":{"threadId":"thr_restart","turn":{"id":"turn_restart","status":"inProgress"}}}'
while :; do sleep 1; done
"#;

#[test]
fn restart_reaps_registered_app_server_and_escalates_uncertain_work()
-> Result<(), Box<dyn std::error::Error>> {
    let root = temporary("hard-crash")?;
    let repository = root.join("repository");
    let home = root.join("home");
    let process_file = root.join("app-server.pid");
    std::fs::create_dir_all(&repository)?;
    std::fs::create_dir_all(&home)?;
    initialize_repository(&repository)?;
    let fake = root.join("codex-blocking");
    std::fs::write(&fake, BLOCKING_APP_SERVER)?;
    std::fs::set_permissions(&fake, Permissions::from_mode(0o700))?;
    let task = root.join("task.json");
    write_task(&task, &repository)?;

    let mut service = start_service(&home, &fake, &process_file)?;
    wait_ready(&mut service)?;
    let submitted = run_cli(&home, &fake, &process_file, &["submit", path(&task)?])?;
    ensure_success(&submitted, "submit")?;
    let handle: TaskHandle = serde_json::from_slice(&submitted.stdout)?;
    wait_for_status(
        &home,
        &fake,
        &process_file,
        &handle.task_id,
        TaskStatus::Running,
    )?;
    let app_server_pid = wait_pid(&process_file)?;

    service.kill()?;
    let _status = service.wait()?;
    assert!(process_alive(app_server_pid));

    let mut restarted = start_service(&home, &fake, &process_file)?;
    wait_ready(&mut restarted)?;
    let projection = wait_for_status(
        &home,
        &fake,
        &process_file,
        &handle.task_id,
        TaskStatus::Escalated,
    )?;
    assert!(projection.summary.contains("uncertain execution state"));
    wait_process_exit(app_server_pid)?;
    let result = run_cli(&home, &fake, &process_file, &["result", &handle.task_id])?;
    ensure_success(&result, "result after restart")?;
    let result: serde_json::Value = serde_json::from_slice(&result.stdout)?;
    assert_eq!(result.get("ready"), Some(&serde_json::Value::Bool(true)));

    let stopped = run_cli(&home, &fake, &process_file, &["stop"])?;
    ensure_success(&stopped, "stop")?;
    let _status = restarted.wait()?;
    std::fs::remove_dir_all(root)?;
    Ok(())
}

fn start_service(home: &Path, fake: &Path, process_file: &Path) -> std::io::Result<Child> {
    Command::new(env!("CARGO_BIN_EXE_spewer"))
        .args([
            "serve",
            "--engine",
            "codex",
            "--max-workers",
            "1",
            "--foreground",
        ])
        .env("HOME", home)
        .env("SPEWER_HOME", home)
        .env("SPEWER_CODEX_BIN", fake)
        .env("SPEWER_TEST_PID_FILE", process_file)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
}

fn run_cli(
    home: &Path,
    fake: &Path,
    process_file: &Path,
    arguments: &[&str],
) -> std::io::Result<Output> {
    Command::new(env!("CARGO_BIN_EXE_spewer"))
        .args(arguments)
        .env("HOME", home)
        .env("SPEWER_HOME", home)
        .env("SPEWER_CODEX_BIN", fake)
        .env("SPEWER_TEST_PID_FILE", process_file)
        .output()
}

fn wait_ready(service: &mut Child) -> Result<(), Box<dyn std::error::Error>> {
    let stdout = service.stdout.take().ok_or("service stdout missing")?;
    let mut reader = BufReader::new(stdout);
    let mut line = String::new();
    reader.read_line(&mut line)?;
    let ready: serde_json::Value = serde_json::from_str(&line).map_err(|error| {
        let mut stderr = String::new();
        if let Some(mut stream) = service.stderr.take() {
            let _read = stream.read_to_string(&mut stderr);
        }
        format!("service readiness failed: {error}; line={line:?}; stderr={stderr}")
    })?;
    if ready.get("ready") != Some(&serde_json::Value::Bool(true)) {
        return Err(format!("service did not report readiness: {line}").into());
    }
    Ok(())
}

fn wait_for_status(
    home: &Path,
    fake: &Path,
    process_file: &Path,
    task_id: &str,
    expected: TaskStatus,
) -> Result<Projection, Box<dyn std::error::Error>> {
    for _attempt in 0..200 {
        let output = run_cli(home, fake, process_file, &["status", task_id])?;
        if output.status.success() {
            let projection: Projection = serde_json::from_slice(&output.stdout)?;
            if projection.status == expected {
                return Ok(projection);
            }
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    Err(format!("task did not reach {expected:?}").into())
}

fn wait_pid(path: &Path) -> Result<u32, Box<dyn std::error::Error>> {
    for _attempt in 0..100 {
        if let Ok(value) = std::fs::read_to_string(path) {
            return Ok(value.trim().parse()?);
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    Err("App Server did not write its process identity".into())
}

fn wait_process_exit(pid: u32) -> Result<(), Box<dyn std::error::Error>> {
    for _attempt in 0..100 {
        if !process_alive(pid) {
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    Err("registered App Server process survived recovery".into())
}

fn process_alive(pid: u32) -> bool {
    Command::new("kill")
        .args(["-0", &pid.to_string()])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
}

fn write_task(path: &Path, repository: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let request = serde_json::json!({
        "protocol_version": "0.1",
        "idempotency_key": "hard-crash-task",
        "objective": "wait for restart recovery",
        "acceptance": ["restart produces an explicit result"],
        "workspace": {"path": repository},
        "context": {"files": [], "notes": []},
        "permissions": {
            "filesystem": "read-only",
            "network": "deny",
            "commands": "engine-policy",
            "environment_allowlist": ["SPEWER_TEST_PID_FILE"],
            "writable_paths": []
        },
        "budgets": {
            "wall_seconds": 30,
            "tokens": 1000,
            "tool_calls": 10,
            "retries": 0,
            "cost_usd": 1.0
        },
        "engine": {"kind": "codex-app-server", "model": "gpt-5.6-luna"},
        "callback": {"mode": "poll", "consumer_id": "play"}
    });
    std::fs::write(path, serde_json::to_vec_pretty(&request)?)?;
    Ok(())
}

fn initialize_repository(path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    command(path, &["git", "init", "-q"])?;
    command(
        path,
        &["git", "config", "user.email", "spewer@example.invalid"],
    )?;
    command(path, &["git", "config", "user.name", "Spewer Test"])?;
    std::fs::write(path.join("README.md"), "fixture\n")?;
    command(path, &["git", "add", "README.md"])?;
    command(path, &["git", "commit", "-qm", "fixture"])?;
    Ok(())
}

fn command(directory: &Path, arguments: &[&str]) -> Result<(), Box<dyn std::error::Error>> {
    let (program, rest) = arguments.split_first().ok_or("command is empty")?;
    let output = Command::new(program)
        .current_dir(directory)
        .args(rest)
        .output()?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).into_owned().into());
    }
    Ok(())
}

fn ensure_success(output: &Output, action: &str) -> Result<(), Box<dyn std::error::Error>> {
    if output.status.success() {
        return Ok(());
    }
    Err(format!(
        "{action} failed: stdout={}; stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
    .into())
}

fn temporary(name: &str) -> Result<PathBuf, Box<dyn std::error::Error>> {
    let nanos = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
    Ok(PathBuf::from("/tmp").join(format!("spwr-{name}-{}-{nanos}", std::process::id())))
}

fn path(value: &Path) -> Result<&str, Box<dyn std::error::Error>> {
    value.to_str().ok_or_else(|| "path is not UTF-8".into())
}
