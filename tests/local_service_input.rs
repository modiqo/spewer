//! End-to-end same-turn human-input continuation test.

#![cfg(unix)]

use spewer::protocol::{TaskHandle, TaskStatus};
use spewer::reducer::Projection;
use std::fs::Permissions;
use std::io::{BufRead, BufReader, Read};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Output, Stdio};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const INPUT_APP_SERVER: &str = r#"#!/bin/sh
trap 'exit 0' TERM
IFS= read -r initialize
printf '%s\n' '{"id":1,"result":{"ready":true}}'
IFS= read -r initialized
IFS= read -r models
printf '%s\n' '{"id":2,"result":{"data":[{"id":"gpt-5.6-luna","model":"gpt-5.6-luna"}]}}'
IFS= read -r thread
printf '%s\n' '{"id":3,"result":{"thread":{"id":"thr_input","sessionId":"ses_input"}}}'
IFS= read -r turn
printf '%s\n' '{"id":4,"result":{"turn":{"id":"turn_input","status":"inProgress","items":[],"error":null}}}'
printf '%s\n' '{"method":"thread/started","params":{"thread":{"id":"thr_input","sessionId":"ses_input"}}}'
printf '%s\n' '{"method":"turn/started","params":{"threadId":"thr_input","turn":{"id":"turn_input","status":"inProgress"}}}'
printf '%s\n' '{"id":99,"method":"item/tool/requestUserInput","params":{"threadId":"thr_input","turnId":"turn_input","itemId":"item_input","isBlocking":true,"questions":[{"id":"dates","header":"Dates","question":"What date range?","isSecret":false}]}}'
IFS= read -r response
printf '%s\n' "$response" > "$FAKE_INPUT_PATH"
printf '%s\n' '{"method":"item/completed","params":{"threadId":"thr_input","turnId":"turn_input","item":{"id":"item_input","type":"agentMessage","status":"completed","text":"Retrieved receipts for August 1–15."}}}'
printf '%s\n' '{"method":"turn/completed","params":{"threadId":"thr_input","turn":{"id":"turn_input","status":"completed","items":[],"error":null}}}'
while IFS= read -r line; do :; done
"#;

#[test]
fn human_response_continues_the_same_codex_task() -> Result<(), Box<dyn std::error::Error>> {
    let root = temporary()?;
    let repository = root.join("repository");
    let home = root.join("home");
    std::fs::create_dir_all(&repository)?;
    std::fs::create_dir_all(&home)?;
    initialize_repository(&repository)?;
    let fake = root.join("codex-input");
    std::fs::write(&fake, INPUT_APP_SERVER)?;
    std::fs::set_permissions(&fake, Permissions::from_mode(0o700))?;
    let task = root.join("input-task.json");
    let response_path = root.join("input-response.json");
    write_task(&task, &repository)?;

    let mut service = start_service(&home, &fake, &response_path)?;
    wait_ready(&mut service)?;
    let submitted = run_cli(&home, &fake, &["submit", path(&task)?])?;
    ensure_success(&submitted, "submit input task")?;
    let handle: TaskHandle = serde_json::from_slice(&submitted.stdout)?;
    let projection = wait_status(&home, &fake, &handle.task_id, TaskStatus::InputRequired)?;
    assert_eq!(
        projection
            .pending_input
            .as_ref()
            .and_then(|value| value.get("request_id")),
        Some(&serde_json::json!(99))
    );

    let response = r#"{"answers":{"dates":{"answers":["August 1–15"]}}}"#;
    let answered = run_cli(
        &home,
        &fake,
        &["respond", &handle.task_id, "99", "--response", response],
    )?;
    ensure_success(&answered, "respond")?;
    let resumed: Projection = serde_json::from_slice(&answered.stdout)?;
    assert_eq!(resumed.task_id, handle.task_id);
    assert_eq!(resumed.status, TaskStatus::Running);
    let completed = wait_status(&home, &fake, &handle.task_id, TaskStatus::Completed)?;
    assert_eq!(completed.task_id, handle.task_id);
    let native: serde_json::Value = serde_json::from_slice(&std::fs::read(&response_path)?)?;
    assert_eq!(native.get("id"), Some(&serde_json::json!(99)));
    assert_eq!(
        native.pointer("/result/answers/dates/answers/0"),
        Some(&serde_json::json!("August 1–15"))
    );

    let stopped = run_cli(&home, &fake, &["stop"])?;
    ensure_success(&stopped, "stop input service")?;
    wait_service_exit(&mut service)?;
    std::fs::remove_dir_all(root)?;
    Ok(())
}

fn start_service(
    home: &Path,
    fake: &Path,
    response_path: &Path,
) -> Result<Child, Box<dyn std::error::Error>> {
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
        .env("FAKE_INPUT_PATH", response_path)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?)
}

fn wait_ready(service: &mut Child) -> Result<(), Box<dyn std::error::Error>> {
    let stdout = service.stdout.take().ok_or("service stdout missing")?;
    let mut reader = BufReader::new(stdout);
    let mut line = String::new();
    reader.read_line(&mut line)?;
    if serde_json::from_str::<serde_json::Value>(&line).is_err() {
        let mut stderr = String::new();
        if let Some(mut stream) = service.stderr.take() {
            stream.read_to_string(&mut stderr)?;
        }
        return Err(format!("service did not become ready: {line:?}; {stderr}").into());
    }
    Ok(())
}

fn wait_status(
    home: &Path,
    fake: &Path,
    task_id: &str,
    expected: TaskStatus,
) -> Result<Projection, Box<dyn std::error::Error>> {
    for _attempt in 0..200 {
        let output = run_cli(home, fake, &["status", task_id])?;
        ensure_success(&output, "status")?;
        let projection: Projection = serde_json::from_slice(&output.stdout)?;
        if projection.status == expected {
            return Ok(projection);
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    Err(format!("task did not become {expected:?}").into())
}

fn run_cli(home: &Path, fake: &Path, arguments: &[&str]) -> std::io::Result<Output> {
    Command::new(env!("CARGO_BIN_EXE_spewer"))
        .args(arguments)
        .env("HOME", home)
        .env("SPEWER_HOME", home)
        .env("SPEWER_CODEX_BIN", fake)
        .output()
}

fn wait_service_exit(service: &mut Child) -> Result<(), Box<dyn std::error::Error>> {
    for _attempt in 0..200 {
        if let Some(status) = service.try_wait()? {
            return if status.success() {
                Ok(())
            } else {
                Err(format!("service failed: {status}").into())
            };
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    service.kill()?;
    let _status = service.wait()?;
    Err("service did not stop after drain".into())
}

fn write_task(path: &Path, repository: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let task = serde_json::json!({
        "protocol_version": "0.1",
        "idempotency_key": "local-service-input",
        "objective": "Retrieve rideshare receipts after asking for a date range.",
        "acceptance": ["The same task completes after input"],
        "workspace": {"path": repository},
        "permissions": {
            "filesystem": "workspace-write",
            "network": "deny",
            "commands": "engine-policy",
            "writable_paths": [],
            "environment_allowlist": ["FAKE_INPUT_PATH"]
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
    Ok(PathBuf::from("/tmp").join(format!("sp-i-{}-{nanos}", std::process::id())))
}

fn path(path: &Path) -> Result<&str, Box<dyn std::error::Error>> {
    path.to_str().ok_or_else(|| "path is not UTF-8".into())
}
