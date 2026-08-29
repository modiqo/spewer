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
while IFS= read -r line; do :; done
"#;

#[test]
fn install_and_live_capsule_binding_need_no_restart() -> Result<(), Box<dyn std::error::Error>> {
    let root = temporary("install")?;
    let home = root.join("home");
    let workspace = root.join("workspace");
    std::fs::create_dir_all(&home)?;
    std::fs::create_dir_all(&workspace)?;
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

fn assert_install_report(
    report: &serde_json::Value,
    config_created: bool,
    service_started: bool,
    capsule_kind: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    if report.get("ready") != Some(&serde_json::Value::Bool(true))
        || report.get("config_created") != Some(&serde_json::Value::Bool(config_created))
        || report.pointer("/service/started") != Some(&serde_json::Value::Bool(service_started))
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
        .env("SPEWER_HOME", home)
        .env("SPEWER_CODEX_BIN", fake)
        .output()
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
