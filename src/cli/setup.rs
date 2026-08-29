//! One-command setup and explicit capsule administration.

use crate::codex::{CodexConfig, DoctorReport, doctor};
use crate::error::{Error, ErrorKind, Result};
use serde::Serialize;
use serde_json::json;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;
use tokio::process::Command;

const INSTALLER_URL: &str = "https://chatgpt.com/codex/install.sh";

#[derive(Debug, Serialize)]
struct InstallReport {
    ready: bool,
    codex_installed: bool,
    codex: DoctorReport,
    config: PathBuf,
    config_created: bool,
    capsules: crate::capsule::CapsuleCatalog,
    service: super::service::DetachedReport,
    next: serde_json::Value,
}

pub(super) async fn install(
    workspace: Option<PathBuf>,
    max_workers: usize,
    skip_codex_install: bool,
) -> Result<()> {
    let (codex, codex_installed) = ensure_codex(skip_codex_install).await?;
    let config_created = crate::config::existing_digest()?.is_none();
    let config = if config_created {
        crate::config::initialize(workspace, None)?
    } else {
        let _existing = crate::config::LocalConfig::load()?;
        crate::config::config_path()?
    };
    let _default = crate::capsule::ensure_default()?;
    let codex_report = doctor(codex).await.map_err(|error| {
        Error::new(
            error.kind(),
            format!(
                "Codex is installed but App Server is not ready: {error}. Run 'codex' to sign in, then rerun 'spewer install'"
            ),
        )
    })?;
    let service =
        super::service::ensure_detached(max_workers, crate::control::default_socket_path()?)
            .await?;
    let capsules = crate::capsule::catalog()?;
    let report = InstallReport {
        ready: true,
        codex_installed,
        codex: codex_report,
        config,
        config_created,
        capsules,
        service,
        next: json!({
            "discover": ["spewer", "capsule", "list"],
            "ask": ["spewer", "ask", "<question>", "--detach"],
            "delegate": ["spewer", "capabilities"]
        }),
    };
    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(())
}

pub(super) fn capsule_list() -> Result<()> {
    println!(
        "{}",
        serde_json::to_string_pretty(&crate::capsule::catalog()?)?
    );
    Ok(())
}

pub(super) fn capsule_bind(capsule_id: &str, skill: &Path) -> Result<()> {
    let manifest = crate::capsule::bind_skill(capsule_id, skill)?;
    println!("{}", serde_json::to_string_pretty(&manifest)?);
    Ok(())
}

pub(super) fn capsule_unbind(capsule_id: &str) -> Result<()> {
    let manifest = crate::capsule::unbind_skill(capsule_id)?;
    println!("{}", serde_json::to_string_pretty(&manifest)?);
    Ok(())
}

async fn ensure_codex(skip_install: bool) -> Result<(CodexConfig, bool)> {
    if let Some(config) = find_codex().await {
        return Ok((config, false));
    }
    if skip_install {
        return Err(Error::new(
            ErrorKind::InvalidInput,
            "Codex CLI was not found; remove --skip-codex-install or install Codex first",
        ));
    }
    tokio::task::spawn_blocking(run_official_installer).await??;
    find_codex().await.map(|config| (config, true)).ok_or_else(|| {
        Error::new(
            ErrorKind::Io,
            "Codex installer completed but the executable was not found; reopen the shell and rerun 'spewer install'",
        )
    })
}

async fn find_codex() -> Option<CodexConfig> {
    for program in codex_candidates() {
        let config = CodexConfig {
            program,
            ..CodexConfig::default()
        };
        if codex_version_works(&config).await {
            return Some(config);
        }
    }
    None
}

fn codex_candidates() -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    if let Some(program) = std::env::var_os("SPEWER_CODEX_BIN") {
        candidates.push(PathBuf::from(program));
    }
    candidates.push(PathBuf::from("codex"));
    if let Some(directory) = std::env::var_os("CODEX_INSTALL_DIR") {
        candidates.push(PathBuf::from(directory).join("codex"));
    }
    if let Some(home) = std::env::var_os("HOME") {
        candidates.push(PathBuf::from(home).join(".local/bin/codex"));
    }
    candidates
}

async fn codex_version_works(config: &CodexConfig) -> bool {
    matches!(
        tokio::time::timeout(
            Duration::from_secs(5),
            Command::new(&config.program).arg("--version").output()
        )
        .await,
        Ok(Ok(output)) if output.status.success()
    )
}

fn run_official_installer() -> Result<()> {
    let mut download = std::process::Command::new("curl")
        .args(["-fsSL", INSTALLER_URL])
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()?;
    let script = download
        .stdout
        .take()
        .ok_or_else(|| Error::new(ErrorKind::Io, "cannot read the Codex installer download"))?;
    let install = std::process::Command::new("sh")
        .env("CODEX_NON_INTERACTIVE", "true")
        .stdin(Stdio::from(script))
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()?;
    let downloaded = download.wait()?;
    if !downloaded.success() || !install.success() {
        return Err(Error::new(ErrorKind::Io, "official Codex installer failed"));
    }
    Ok(())
}
