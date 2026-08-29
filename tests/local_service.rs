//! End-to-end CLI, control socket, scheduler, and App Server process test.

#![cfg(unix)]

use spewer::delivery::OutboxMessage;
use spewer::protocol::{DEFAULT_MODEL, TaskHandle, TaskStatus};
use spewer::reducer::Projection;
use spewer::store::Observation;
use spewer::supervisor::SupervisorLoad;
use std::fs::Permissions;
use std::io::{BufRead, BufReader, Read, Write};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Output, Stdio};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const FAKE_APP_SERVER: &str = r#"#!/bin/sh
trap 'exit 0' TERM
IFS= read -r initialize
printf '%s\n' '{"id":1,"result":{"ready":true}}'
IFS= read -r initialized
IFS= read -r models
printf '%s\n' '{"id":2,"result":{"data":[{"id":"gpt-5.6-luna","model":"gpt-5.6-luna"}]}}'
IFS= read -r thread
printf '%s\n' '{"id":3,"result":{"thread":{"id":"thr_local","sessionId":"ses_local"}}}'
IFS= read -r turn
printf '%s\n' '{"id":4,"result":{"turn":{"id":"turn_local","status":"inProgress","items":[],"error":null}}}'
printf '%s\n' '{"method":"thread/started","params":{"thread":{"id":"thr_local","sessionId":"ses_local"}}}'
printf '%s\n' '{"method":"turn/started","params":{"threadId":"thr_local","turn":{"id":"turn_local","status":"inProgress"}}}'
printf '%s\n' '{"method":"item/completed","params":{"threadId":"thr_local","turnId":"turn_local","item":{"id":"item_local","type":"agentMessage","status":"completed","text":"Local service completed the turn."}}}'
printf '%s\n' '{"method":"thread/tokenUsage/updated","params":{"threadId":"thr_local","turnId":"turn_local","tokenUsage":{"total":{"inputTokens":20,"cachedInputTokens":5,"outputTokens":7,"reasoningOutputTokens":2,"totalTokens":29}}}}'
printf '%s\n' '{"method":"turn/completed","params":{"threadId":"thr_local","turn":{"id":"turn_local","status":"completed","items":[],"error":null}}}'
while IFS= read -r line; do :; done
"#;

#[test]
fn detached_service_returns_json_and_stays_ready() -> Result<(), Box<dyn std::error::Error>> {
    let root = temporary("d")?;
    let home = root.join("home");
    std::fs::create_dir_all(&home)?;
    let unused_fake = root.join("unused-codex");

    let started = run_cli(
        &home,
        &unused_fake,
        &["serve", "--engine", "codex", "--max-workers", "1"],
    )?;
    ensure_success(&started, "detached serve")?;
    let started: serde_json::Value = serde_json::from_slice(&started.stdout)?;
    assert_eq!(started.get("ready"), Some(&serde_json::Value::Bool(true)));
    assert_eq!(started.get("started"), Some(&serde_json::Value::Bool(true)));
    assert_eq!(
        started.get("mode").and_then(serde_json::Value::as_str),
        Some("detached")
    );
    assert!(
        started
            .get("pid")
            .and_then(serde_json::Value::as_u64)
            .is_some()
    );
    assert_eq!(
        started
            .pointer("/load/max_workers")
            .and_then(serde_json::Value::as_u64),
        Some(1)
    );

    let load = run_cli(&home, &unused_fake, &["load"])?;
    ensure_success(&load, "load detached service")?;
    let repeated = run_cli(
        &home,
        &unused_fake,
        &["serve", "--engine", "codex", "--json"],
    )?;
    ensure_success(&repeated, "repeated detached serve")?;
    let repeated: serde_json::Value = serde_json::from_slice(&repeated.stdout)?;
    assert_eq!(
        repeated.get("started"),
        Some(&serde_json::Value::Bool(false))
    );
    assert!(repeated.get("pid").is_some_and(serde_json::Value::is_null));

    let log = home.join("spewer-service.log");
    assert!(log.is_file());
    assert_private_file(&log)?;
    let stopped = run_cli(&home, &unused_fake, &["stop"])?;
    ensure_success(&stopped, "stop detached service")?;
    wait_removed(&home.join("spewer.sock"))?;
    std::fs::remove_dir_all(root)?;
    Ok(())
}

#[test]
fn local_service_completes_a_default_luna_turn() -> Result<(), Box<dyn std::error::Error>> {
    let root = temporary("e2e")?;
    let repository = root.join("repository");
    let home = root.join("home");
    std::fs::create_dir_all(&repository)?;
    std::fs::create_dir_all(&home)?;
    initialize_repository(&repository)?;
    let fake = root.join("codex-fake");
    std::fs::write(&fake, FAKE_APP_SERVER)?;
    std::fs::set_permissions(&fake, Permissions::from_mode(0o700))?;
    let task = root.join("task.json");
    write_task(&task, &repository)?;

    let mut service = start_service(&home, &fake)?;
    wait_ready(&mut service)?;
    assert_capabilities(&home, &fake)?;
    let submitted = run_cli(&home, &fake, &["submit", path(&task)?])?;
    ensure_success(&submitted, "submit")?;
    let handle: TaskHandle = serde_json::from_slice(&submitted.stdout).map_err(|error| {
        format!(
            "submit output was not a handle: {error}; stdout={}; stderr={}",
            String::from_utf8_lossy(&submitted.stdout),
            String::from_utf8_lossy(&submitted.stderr)
        )
    })?;
    let load = run_cli(&home, &fake, &["load"])?;
    ensure_success(&load, "load")?;
    let load: SupervisorLoad = serde_json::from_slice(&load.stdout)?;
    assert_eq!(load.max_workers, 1);
    assert_eq!(load.accepted_tasks, 1);
    let projection = wait_terminal(&home, &fake, &handle.task_id)?;
    assert_eq!(projection.status, TaskStatus::Completed);
    let observed = run_cli(&home, &fake, &["observe", &handle.task_id, "--after", "1"])?;
    ensure_success(&observed, "observe")?;
    let observed: Observation = serde_json::from_slice(&observed.stdout)?;
    assert_eq!(observed.next_cursor, projection.event_seq);
    assert!(observed.events.iter().all(|event| event.seq > 1));
    let result = run_cli(&home, &fake, &["result", &handle.task_id])?;
    ensure_success(&result, "result")?;
    let result: serde_json::Value = serde_json::from_slice(&result.stdout)?;
    assert_eq!(result.get("ready"), Some(&serde_json::Value::Bool(true)));

    let outbox = run_cli(&home, &fake, &["outbox", "play"])?;
    ensure_success(&outbox, "outbox")?;
    let line = std::str::from_utf8(&outbox.stdout)?
        .lines()
        .next()
        .ok_or("outbox did not contain a callback")?;
    let message: OutboxMessage = serde_json::from_str(line)?;
    assert_eq!(message.receipt.engine.requested_model, DEFAULT_MODEL);
    assert_eq!(message.receipt.engine.observed_models, [DEFAULT_MODEL]);
    assert_eq!(message.receipt.usage.output_tokens, Some(7));

    let acknowledged = run_cli(&home, &fake, &["ack", &message.message_id, "play"])?;
    ensure_success(&acknowledged, "ack")?;
    let empty = run_cli(&home, &fake, &["outbox", "play"])?;
    ensure_success(&empty, "outbox after ack")?;
    assert!(empty.stdout.is_empty());
    let retained = run_cli(&home, &fake, &["result", &handle.task_id])?;
    ensure_success(&retained, "result after ack")?;
    let retained: serde_json::Value = serde_json::from_slice(&retained.stdout)?;
    assert_eq!(retained.get("ready"), Some(&serde_json::Value::Bool(true)));

    let initialized = run_cli(&home, &fake, &["init", "--workspace", path(&repository)?])?;
    ensure_success(&initialized, "init for detached ask")?;
    let detached = run_cli(&home, &fake, &["ask", "What is two plus two?", "--detach"])?;
    ensure_success(&detached, "detached ask")?;
    let detached: serde_json::Value = serde_json::from_slice(&detached.stdout)?;
    let detached_id = detached
        .pointer("/handle/task_id")
        .and_then(serde_json::Value::as_str)
        .ok_or("detached handle omitted task_id")?;
    let expected = serde_json::json!(["spewer", "observe", detached_id, "--after", "0"]);
    assert_eq!(detached.pointer("/next/observe"), Some(&expected));
    assert_eq!(
        wait_terminal(&home, &fake, detached_id)?.status,
        TaskStatus::Completed
    );
    let detached_result = run_cli(&home, &fake, &["result", detached_id])?;
    ensure_success(&detached_result, "detached result")?;
    let detached_result: serde_json::Value = serde_json::from_slice(&detached_result.stdout)?;
    let detached_message: OutboxMessage = serde_json::from_value(
        detached_result
            .pointer("/result/message")
            .cloned()
            .ok_or("detached result omitted its callback")?,
    )?;
    assert_eq!(detached_message.mode, "poll");
    assert_eq!(detached_message.task_id, detached_id);
    let detached_ack = run_cli(
        &home,
        &fake,
        &["ack", &detached_message.message_id, "spewer-ask"],
    )?;
    ensure_success(&detached_ack, "detached ack")?;

    let stopped = run_cli(&home, &fake, &["stop"])?;
    ensure_success(&stopped, "stop")?;
    wait_service_exit(&mut service)?;
    assert!(!home.join("spewer.sock").exists());
    std::fs::remove_dir_all(root)?;
    Ok(())
}

#[test]
fn ask_initializes_infers_answers_and_acknowledges() -> Result<(), Box<dyn std::error::Error>> {
    let root = temporary("ask")?;
    let repository = root.join("repository");
    let home = root.join("home");
    std::fs::create_dir_all(&repository)?;
    std::fs::create_dir_all(&home)?;
    initialize_repository(&repository)?;
    let fake = root.join("codex-fake");
    std::fs::write(&fake, FAKE_APP_SERVER)?;
    std::fs::set_permissions(&fake, Permissions::from_mode(0o700))?;

    let initialized = run_cli(&home, &fake, &["init", "--workspace", path(&repository)?])?;
    ensure_success(&initialized, "init")?;
    let initialized: serde_json::Value = serde_json::from_slice(&initialized.stdout)?;
    assert_eq!(
        initialized
            .get("initialized")
            .and_then(serde_json::Value::as_bool),
        Some(true)
    );

    let original = std::fs::read(home.join(".spewer/config.json"))?;
    let declined = run_cli_with_input(&home, &fake, &["init", "--overwrite"], "n\n")?;
    ensure_success(&declined, "declined overwrite")?;
    assert_eq!(std::fs::read(home.join(".spewer/config.json"))?, original);
    assert!(String::from_utf8(declined.stderr)?.contains("[Y/n]"));
    let approved = run_cli_with_input(&home, &fake, &["init", "--overwrite"], "\n")?;
    ensure_success(&approved, "approved overwrite")?;

    let asked = run_cli(&home, &fake, &["ask", "What is two plus two?"])?;
    ensure_success(&asked, "ask")?;
    let asked_json: serde_json::Value = serde_json::from_slice(&asked.stdout)?;
    assert_eq!(
        asked_json.get("answer").and_then(serde_json::Value::as_str),
        Some("Local service completed the turn.")
    );
    assert_eq!(
        asked_json
            .pointer("/receipt/engine/requested_model")
            .and_then(serde_json::Value::as_str),
        Some(DEFAULT_MODEL)
    );

    let asked_text = run_cli(&home, &fake, &["ask", "What is two plus two?", "--text"])?;
    ensure_success(&asked_text, "ask text")?;
    assert!(String::from_utf8(asked_text.stdout)?.contains("Local service completed the turn."));
    let telemetry = String::from_utf8(asked_text.stderr)?;
    assert!(telemetry.contains("model=gpt-5.6-luna"));
    assert!(telemetry.contains("output=7"));

    let outbox = run_cli(&home, &fake, &["outbox", "spewer-ask"])?;
    ensure_success(&outbox, "ask outbox")?;
    assert!(outbox.stdout.is_empty());
    std::fs::remove_dir_all(root)?;
    Ok(())
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

fn assert_capabilities(home: &Path, fake: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let output = run_cli(home, fake, &["capabilities"])?;
    ensure_success(&output, "capabilities")?;
    let capabilities: serde_json::Value = serde_json::from_slice(&output.stdout)?;
    assert_eq!(
        capabilities.get("operations"),
        Some(&serde_json::json!([
            "capabilities",
            "submit",
            "observe",
            "result",
            "cancel",
            "acknowledge",
            "load",
            "stop"
        ]))
    );
    Ok(())
}

fn wait_ready(service: &mut Child) -> Result<(), Box<dyn std::error::Error>> {
    let stdout = service.stdout.take().ok_or("service stdout missing")?;
    let mut reader = BufReader::new(stdout);
    let mut line = String::new();
    reader.read_line(&mut line)?;
    let ready: serde_json::Value = match serde_json::from_str(&line) {
        Ok(ready) => ready,
        Err(error) => {
            let mut stderr = String::new();
            if let Some(mut stream) = service.stderr.take() {
                stream.read_to_string(&mut stderr)?;
            }
            return Err(format!(
                "service readiness was not JSON: {error}; line={line:?}; status={:?}; stderr={stderr}",
                service.try_wait()
            )
            .into());
        }
    };
    if ready.get("ready").and_then(serde_json::Value::as_bool) != Some(true) {
        return Err(format!("service did not become ready: {line}").into());
    }
    Ok(())
}

fn wait_terminal(
    home: &Path,
    fake: &Path,
    task_id: &str,
) -> Result<Projection, Box<dyn std::error::Error>> {
    for _attempt in 0..200 {
        let output = run_cli(home, fake, &["status", task_id])?;
        ensure_success(&output, "status")?;
        let projection: Projection = serde_json::from_slice(&output.stdout)?;
        if projection.status.is_terminal() {
            return Ok(projection);
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    Err("task did not become terminal".into())
}

fn run_cli(home: &Path, fake: &Path, arguments: &[&str]) -> std::io::Result<Output> {
    Command::new(env!("CARGO_BIN_EXE_spewer"))
        .args(arguments)
        .env("HOME", home)
        .env("SPEWER_HOME", home)
        .env("SPEWER_CODEX_BIN", fake)
        .output()
}

fn run_cli_with_input(
    home: &Path,
    fake: &Path,
    arguments: &[&str],
    input: &str,
) -> std::io::Result<Output> {
    let mut child = Command::new(env!("CARGO_BIN_EXE_spewer"))
        .args(arguments)
        .env("HOME", home)
        .env("SPEWER_HOME", home)
        .env("SPEWER_CODEX_BIN", fake)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(input.as_bytes())?;
    }
    child.wait_with_output()
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

fn wait_removed(path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    for _attempt in 0..200 {
        if !path.exists() {
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    Err(format!("path was not removed: {}", path.display()).into())
}

#[cfg(unix)]
fn assert_private_file(path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    if std::fs::metadata(path)?.permissions().mode() & 0o777 != 0o600 {
        return Err("detached service log is not owner-private".into());
    }
    Ok(())
}

fn write_task(path: &Path, repository: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let task = serde_json::json!({
        "protocol_version": "0.1",
        "idempotency_key": "local-service-default-luna",
        "objective": "Return a short confirmation without changing files.",
        "acceptance": ["The turn completes"],
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

fn temporary(name: &str) -> Result<PathBuf, Box<dyn std::error::Error>> {
    let nanos = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
    Ok(std::env::temp_dir().join(format!("spewer-{name}-{}-{nanos}", std::process::id())))
}

fn path(path: &Path) -> Result<&str, Box<dyn std::error::Error>> {
    path.to_str().ok_or_else(|| "path is not UTF-8".into())
}
