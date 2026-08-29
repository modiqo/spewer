//! One-command setup and live capsule discovery contract.

#![cfg(unix)]

use std::fs::Permissions;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const FAKE_CODEX: &str = r#"#!/bin/sh
if [ "${1:-}" = "--version" ]; then
  printf '%s\n' 'codex-cli 1.0.0-test'
  exit 0
fi
trap 'exit 0' TERM
IFS= read -r initialize
printf '%s\n' '{"id":1,"result":{"ready":true}}'
IFS= read -r initialized
IFS= read -r models
printf '%s\n' '{"id":2,"result":{"data":[{"id":"gpt-5.6-luna","model":"gpt-5.6-luna"}]}}'
if ! IFS= read -r thread; then exit 0; fi
printf '%s\n' '{"id":3,"result":{"thread":{"id":"thr_capsule","sessionId":"ses_capsule"}}}'
IFS= read -r turn
printf '%s\n' "$turn" > "$FAKE_TURN_PATH"
printf '%s\n' '{"id":4,"result":{"turn":{"id":"turn_capsule","status":"inProgress","items":[],"error":null}}}'
printf '%s\n' '{"method":"turn/started","params":{"threadId":"thr_capsule","turn":{"id":"turn_capsule","status":"inProgress"}}}'
printf '%s\n' '{"method":"item/completed","params":{"threadId":"thr_capsule","turnId":"turn_capsule","item":{"id":"item_capsule","type":"agentMessage","status":"completed","text":"Specialized capsule completed the turn."}}}'
printf '%s\n' '{"method":"turn/completed","params":{"threadId":"thr_capsule","turn":{"id":"turn_capsule","status":"completed","items":[],"error":null}}}'
while IFS= read -r line; do :; done
"#;

#[test]
fn install_and_live_capsule_binding_need_no_restart() -> Result<(), Box<dyn std::error::Error>> {
    let root = temporary("install")?;
    let home = root.join("home");
    let workspace = root.join("workspace");
    std::fs::create_dir_all(&home)?;
    std::fs::create_dir_all(&workspace)?;
    initialize_repository(&workspace)?;
    let fake = root.join("codex-fake");
    std::fs::write(&fake, FAKE_CODEX)?;
    std::fs::set_permissions(&fake, Permissions::from_mode(0o700))?;

    let installed = run_cli(
        &home,
        &fake,
        &[
            "install",
            "--workspace",
            path(&workspace)?,
            "--skip-codex-install",
        ],
    )?;
    ensure_success(&installed, "install")?;
    let installed: serde_json::Value = serde_json::from_slice(&installed.stdout)?;
    assert_install_report(&installed, true, true, "generic")?;

    let before = capabilities(&home, &fake)?;
    let before_revision = before
        .get("capsule_revision")
        .and_then(serde_json::Value::as_str)
        .ok_or("capability revision missing")?
        .to_owned();
    assert_eq!(
        before
            .pointer("/capsules/0/kind")
            .and_then(serde_json::Value::as_str),
        Some("generic")
    );
    assert_default_runtime_capabilities(&before);

    let skill = root.join("skill/SKILL.md");
    let skill_parent = skill.parent().ok_or("skill parent missing")?;
    std::fs::create_dir_all(skill_parent)?;
    std::fs::write(
        &skill,
        "---\nname: review\ndescription: Review bounded changes\nversion: 1\n---\nReview.\n",
    )?;
    let bound = run_cli(&home, &fake, &["capsule", "bind", "default", path(&skill)?])?;
    ensure_success(&bound, "capsule bind")?;

    let after = capabilities(&home, &fake)?;
    assert_ne!(
        after
            .get("capsule_revision")
            .and_then(serde_json::Value::as_str),
        Some(before_revision.as_str())
    );
    assert_eq!(
        after
            .pointer("/capsules/0/kind")
            .and_then(serde_json::Value::as_str),
        Some("specialized")
    );
    assert_eq!(
        after
            .pointer("/capsules/0/skill/name")
            .and_then(serde_json::Value::as_str),
        Some("review")
    );
    assert!(after.pointer("/capsules/0/skill/source").is_none());
    dispatch_specialized(&home, &fake, &workspace, &after)?;

    let repeated = run_cli(
        &home,
        &fake,
        &[
            "install",
            "--workspace",
            path(&workspace)?,
            "--skip-codex-install",
        ],
    )?;
    ensure_success(&repeated, "repeated install")?;
    let repeated: serde_json::Value = serde_json::from_slice(&repeated.stdout)?;
    assert_install_report(&repeated, false, false, "specialized")?;

    let unbound = run_cli(&home, &fake, &["capsule", "unbind", "default"])?;
    ensure_success(&unbound, "capsule unbind")?;
    let restored = capabilities(&home, &fake)?;
    assert_eq!(
        restored
            .pointer("/capsules/0/kind")
            .and_then(serde_json::Value::as_str),
        Some("generic")
    );
    assert_eq!(
        restored
            .get("capsule_revision")
            .and_then(serde_json::Value::as_str),
        Some(before_revision.as_str())
    );

    let stopped = run_cli(&home, &fake, &["stop"])?;
    ensure_success(&stopped, "stop")?;
    wait_removed(&home.join("spewer.sock"))?;
    std::fs::remove_dir_all(root)?;
    Ok(())
}

fn assert_default_runtime_capabilities(capabilities: &serde_json::Value) {
    assert_eq!(
        capabilities
            .pointer("/capsules/0/network")
            .and_then(serde_json::Value::as_bool),
        Some(true)
    );
    assert_eq!(
        capabilities
            .pointer("/capsules/0/tools")
            .and_then(serde_json::Value::as_array)
            .map(Vec::len),
        Some(2)
    );
}

fn dispatch_specialized(
    home: &Path,
    fake: &Path,
    workspace: &Path,
    capabilities: &serde_json::Value,
) -> Result<(), Box<dyn std::error::Error>> {
    let capsule = capabilities
        .pointer("/capsules/0")
        .ok_or("specialized capsule missing")?;
    let task = serde_json::json!({
        "protocol_version": "0.1",
        "idempotency_key": "capsule-dispatch-e2e",
        "objective": "Apply the bound review skill without changing files.",
        "acceptance": ["The specialized turn completes"],
        "workspace": {"path": workspace},
        "permissions": {
            "filesystem": "read-only",
            "network": "deny",
            "commands": "engine-policy",
            "environment_allowlist": ["FAKE_TURN_PATH"]
        },
        "budgets": {
            "wall_seconds": 30,
            "tokens": 1000,
            "tool_calls": 10,
            "retries": 0,
            "cost_usd": 1.0
        },
        "engine": {"kind": "codex-app-server", "model": "ignored-by-delegate"},
        "callback": {"mode": "poll", "consumer_id": "capsule-test"}
    });
    let task_path = home.join("capsule-task.json");
    std::fs::write(&task_path, serde_json::to_vec_pretty(&task)?)?;
    let missing = run_cli(
        home,
        fake,
        &["delegate", path(&task_path)?, "--capsule", "missing"],
    )?;
    assert!(!missing.status.success());
    let delegated = run_cli(
        home,
        fake,
        &["delegate", path(&task_path)?, "--capsule", "default"],
    )?;
    ensure_success(&delegated, "capsule delegate")?;
    let delegation: serde_json::Value = serde_json::from_slice(&delegated.stdout)?;
    assert_eq!(
        delegation
            .pointer("/capsule/revision")
            .and_then(serde_json::Value::as_str),
        capsule.get("revision").and_then(serde_json::Value::as_str)
    );
    let task_id = delegation
        .pointer("/handle/task_id")
        .and_then(serde_json::Value::as_str)
        .ok_or("task id missing")?;
    let result = wait_check(home, fake, task_id)?;
    assert_eq!(
        result
            .pointer("/result/message/receipt/capsule/skill/name")
            .and_then(serde_json::Value::as_str),
        Some("review")
    );
    assert_eq!(
        result
            .pointer("/result/message/receipt/capsule/revision")
            .and_then(serde_json::Value::as_str),
        capsule.get("revision").and_then(serde_json::Value::as_str)
    );
    let turn = std::fs::read_to_string(home.join("turn.json"))?;
    assert!(turn.contains("Review."));
    Ok(())
}

fn wait_check(
    home: &Path,
    fake: &Path,
    task_id: &str,
) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    for _attempt in 0..200 {
        let output = run_cli(home, fake, &["check", task_id, "--after", "0"])?;
        ensure_success(&output, "capsule check")?;
        let result: serde_json::Value = serde_json::from_slice(&output.stdout)?;
        if result.get("ready") == Some(&serde_json::Value::Bool(true)) {
            return Ok(result);
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    Err("capsule task did not finish".into())
}

fn assert_install_report(
    report: &serde_json::Value,
    config_created: bool,
    service_started: bool,
    capsule_kind: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    if report.get("ready") != Some(&serde_json::Value::Bool(true))
        || report.get("config_created") != Some(&serde_json::Value::Bool(config_created))
        || report.pointer("/service/started") != Some(&serde_json::Value::Bool(service_started))
        || report.pointer("/frontier_skill/created")
            != Some(&serde_json::Value::Bool(config_created))
        || report
            .pointer("/frontier_skill/path")
            .and_then(serde_json::Value::as_str)
            .is_none_or(|path| !path.ends_with("skills/spewer-delegation/SKILL.md"))
        || report
            .pointer("/capsules/capsules/0/kind")
            .and_then(serde_json::Value::as_str)
            != Some(capsule_kind)
    {
        return Err(format!("unexpected install report: {report}").into());
    }
    Ok(())
}

fn capabilities(home: &Path, fake: &Path) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    let output = run_cli(home, fake, &["capabilities"])?;
    ensure_success(&output, "capabilities")?;
    Ok(serde_json::from_slice(&output.stdout)?)
}

fn run_cli(home: &Path, fake: &Path, arguments: &[&str]) -> std::io::Result<Output> {
    Command::new(env!("CARGO_BIN_EXE_spewer"))
        .args(arguments)
        .env("HOME", home)
        .env("CODEX_HOME", home.join(".codex"))
        .env("SPEWER_HOME", home)
        .env("SPEWER_CODEX_BIN", fake)
        .env("FAKE_TURN_PATH", home.join("turn.json"))
        .output()
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

fn wait_removed(path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    for _attempt in 0..200 {
        if !path.exists() {
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    Err(format!("path was not removed: {}", path.display()).into())
}

fn path(path: &Path) -> Result<&str, Box<dyn std::error::Error>> {
    path.to_str().ok_or_else(|| "test path is not UTF-8".into())
}

fn temporary(name: &str) -> Result<PathBuf, Box<dyn std::error::Error>> {
    let nanos = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
    for attempt in 0..100_u8 {
        let candidate = PathBuf::from("/tmp").join(format!(
            "spewer-{name}-{}-{nanos}-{attempt}",
            std::process::id()
        ));
        match std::fs::create_dir(&candidate) {
            Ok(()) => return Ok(candidate),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(error.into()),
        }
    }
    Err("could not create a unique test directory".into())
}
