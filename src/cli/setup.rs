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
    frontier_skill: super::skill_install::SkillInstallReport,
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
    let frontier_skill = super::skill_install::install()?;
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
        frontier_skill,
        service,
        next: json!({
            "discover": ["spewer", "capsule", "list"],
            "ask": ["spewer", "ask", "<question>", "--detach"],
            "delegate": ["spewer", "delegate", "<task.json>", "--capsule", "default"]
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

pub(super) fn capsule_show(capsule_id: Option<&str>) -> Result<()> {
    let config = crate::config::LocalConfig::load()?;
    let capsule_id = match capsule_id {
        Some(capsule_id) => capsule_id,
        None => &config.default_capsule,
    };
    let catalog = crate::capsule::catalog()?;
    let capsule = catalog
        .capsules
        .iter()
        .find(|capsule| capsule.id == capsule_id)
        .ok_or_else(|| {
            Error::new(
                ErrorKind::InvalidInput,
                format!("capsule {capsule_id} is not installed"),
            )
        })?;
    println!(
        "{}",
        serde_json::to_string_pretty(&capsule_guidance(capsule, &config.default_capsule))?
    );
    Ok(())
}

pub(super) fn capsule_default(capsule_id: &str) -> Result<()> {
    let catalog = crate::capsule::catalog()?;
    let capsule = catalog
        .capsules
        .iter()
        .find(|capsule| capsule.id == capsule_id)
        .ok_or_else(|| {
            Error::new(
                ErrorKind::InvalidInput,
                format!("capsule {capsule_id} is not installed"),
            )
        })?;
    let config = crate::config::set_default_capsule(capsule_id)?;
    println!(
        "{}",
        serde_json::to_string_pretty(&capsule_guidance(capsule, &config.default_capsule))?
    );
    Ok(())
}

fn capsule_guidance(
    capsule: &crate::capsule::CapsuleAdvertisement,
    default_capsule: &str,
) -> serde_json::Value {
    let is_default = capsule.id == default_capsule;
    let mut ask = vec!["spewer", "ask", "<question>"]
        .into_iter()
        .map(str::to_owned)
        .collect::<Vec<_>>();
    if !is_default {
        ask.extend(["--capsule".to_owned(), capsule.id.clone()]);
    }
    let web_available = capsule.network && capsule.tools.iter().any(|tool| tool == "web_search");
    let web_example = web_available.then(|| {
        let mut command = ask.clone();
        command.push("--web".to_owned());
        command
    });
    let danger_available = capsule.engine.kind == "codex-app-server";
    let danger_example = danger_available.then(|| {
        let mut command = ask.clone();
        command.push("--danger-full-access".to_owned());
        command
    });
    json!({
        "default": is_default,
        "capability_source": "current process",
        "capsule": capsule,
        "ask": {
            "command": ask,
            "request_authority": {
                "--web": {
                    "available": web_available,
                    "meaning": "allow this task to use the advertised web_search tool"
                },
                "--danger-full-access": {
                    "available": danger_available,
                    "alias": "--no-sandbox",
                    "meaning": "disable the Codex sandbox and allow filesystem plus network access for this task"
                }
            },
            "output": {
                "default": "answer text plus telemetry",
                "--json": "structured answer and receipt",
                "--detach": "durable task handle"
            },
            "web_example": web_example,
            "danger_example": danger_example
        },
        "detached_service_check": ["spewer", "capabilities"]
    })
}

pub(super) async fn capsule_add(capsule_id: &str, engine: &str, model: &str) -> Result<()> {
    let (description, resolved_model) = match engine {
        crate::ollama::ENGINE_KIND => {
            let doctor =
                crate::ollama::doctor(crate::ollama::OllamaConfig::default(), Some(model)).await?;
            let resolved_model = doctor.required_model.ok_or_else(|| {
                Error::new(
                    ErrorKind::EngineProtocol,
                    "Ollama discovery omitted the required model",
                )
            })?;
            (
                format!("Read-only local inference through Ollama model {resolved_model}"),
                resolved_model,
            )
        }
        "codex-app-server" => {
            let _doctor = doctor(CodexConfig::default()).await?;
            (
                format!("Bounded work through Codex App Server model {model}"),
                model.to_owned(),
            )
        }
        other => {
            return Err(Error::new(
                ErrorKind::InvalidInput,
                format!("unsupported capsule engine {other}; use codex-app-server or ollama"),
            ));
        }
    };
    let manifest = crate::capsule::create(
        capsule_id,
        description,
        crate::protocol::EngineRequest {
            kind: engine.to_owned(),
            model: resolved_model,
            effort: None,
        },
    )?;
    println!("{}", serde_json::to_string_pretty(&manifest)?);
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
