//! Attached and detached questions over the same durable task protocol.

use crate::codex::CodexConfig;
use crate::error::{Error, ErrorKind, Result};
use crate::protocol::{Receipt, TaskRequest};
use crate::runner::{RunResult, run_codex_durable};
use crate::store::Database;
use serde_json::json;
use std::future::Future;
use std::io::{BufRead, IsTerminal, Write};
use std::path::PathBuf;
use std::time::Duration;
use tokio::time::Instant;

use super::parse::CliCommand;

pub(super) async fn initialize(workspace: Option<PathBuf>, overwrite: bool) -> Result<()> {
    let expected = if overwrite {
        tokio::task::spawn_blocking(crate::config::existing_digest).await??
    } else {
        None
    };
    if expected.is_some() {
        let path = crate::config::config_path()?;
        let approved = tokio::task::spawn_blocking(move || confirm_overwrite(&path)).await??;
        if !approved {
            println!(
                "{}",
                serde_json::to_string_pretty(&json!({"initialized": false, "cancelled": true}))?
            );
            return Ok(());
        }
    }
    let path = tokio::task::spawn_blocking(move || crate::config::initialize(workspace, expected))
        .await??;
    println!(
        "{}",
        serde_json::to_string_pretty(&json!({
            "initialized": true,
            "config": path,
            "next": "spewer ask \"What is two plus two?\""
        }))?
    );
    Ok(())
}

pub(super) async fn ask(command: CliCommand) -> Result<()> {
    let CliCommand::Ask {
        question,
        workspace,
        capsule_id,
        web,
        danger_full_access,
        text: text_output,
        detach,
        socket,
    } = command
    else {
        return Err(Error::new(
            ErrorKind::InvalidInput,
            "question runner received a non-ask command",
        ));
    };
    let config = tokio::task::spawn_blocking(crate::config::LocalConfig::load).await??;
    let mut request = config.infer_question(&question, workspace)?;
    let capsule_id = match capsule_id {
        Some(capsule_id) => capsule_id,
        None => config.default_capsule.clone(),
    };
    let detached_socket = if detach {
        Some(match socket {
            Some(path) => path,
            None => crate::control::default_socket_path()?,
        })
    } else {
        None
    };
    let selected = match detached_socket.as_ref() {
        Some(socket) => select_service_capsule(&mut request, &capsule_id, socket).await?,
        None => select_capsule(&mut request, &capsule_id)?,
    };
    if danger_full_access {
        grant_danger_full_access(&mut request, &selected.engine.kind)?;
    }
    if web {
        if !selected.network || !selected.tools.iter().any(|tool| tool == "web_search") {
            return Err(Error::new(
                ErrorKind::InvalidInput,
                format!(
                    "capsule {} does not advertise web_search; set OLLAMA_API_KEY before starting Spewer",
                    selected.id
                ),
            ));
        }
        "allow".clone_into(&mut request.permissions.network);
    }
    request.validate()?;
    if detach {
        "poll".clone_into(&mut request.callback.mode);
        return submit_detached(request, detached_socket).await;
    }
    Box::pin(run_attached(request, text_output)).await
}

fn grant_danger_full_access(request: &mut TaskRequest, engine_kind: &str) -> Result<()> {
    if engine_kind != "codex-app-server" {
        return Err(Error::new(
            ErrorKind::InvalidInput,
            "--danger-full-access requires a Codex App Server capsule",
        ));
    }
    "danger-full-access".clone_into(&mut request.permissions.filesystem);
    "allow".clone_into(&mut request.permissions.network);
    request.context.notes.push(
        "The user explicitly granted this task unsandboxed filesystem and network access."
            .to_owned(),
    );
    Ok(())
}

async fn select_service_capsule(
    request: &mut TaskRequest,
    capsule_id: &str,
    socket: &std::path::Path,
) -> Result<crate::capsule::CapsuleAdvertisement> {
    let capabilities = crate::control::capabilities(socket.to_owned()).await?;
    let capsule = capabilities
        .capsules
        .into_iter()
        .find(|capsule| capsule.id == capsule_id)
        .ok_or_else(|| {
            Error::new(
                ErrorKind::InvalidInput,
                format!("capsule {capsule_id} is not advertised by the service"),
            )
        })?;
    request.engine = capsule.engine.clone();
    request.capsule = Some(crate::capsule::CapsuleRequest {
        id: capsule.id.clone(),
        revision: capsule.revision.clone(),
        binding: None,
    });
    Ok(capsule)
}

async fn run_attached(request: TaskRequest, text_output: bool) -> Result<()> {
    let task_id = request
        .task_id
        .clone()
        .ok_or_else(|| Error::new(ErrorKind::InvalidInput, "ask task has no identifier"))?;
    let database = Database::open(Database::default_path()?).await?;
    let engine = request.engine.kind.clone();
    let outcome = match engine.as_str() {
        "codex-app-server" => {
            let run = run_codex_durable(request, CodexConfig::default(), &database);
            Box::pin(wait_with_progress(run, &database, &task_id)).await
        }
        crate::ollama::ENGINE_KIND => {
            let run = crate::runner::run_ollama_durable(
                request,
                crate::ollama::OllamaConfig::default(),
                &database,
            );
            Box::pin(wait_with_progress(run, &database, &task_id)).await
        }
        other => Err(Error::new(
            ErrorKind::InvalidInput,
            format!("ask cannot dispatch engine {other}"),
        )),
    };
    let result = match outcome {
        Ok(result) => result,
        Err(error) => {
            let _closed = database.close().await;
            return Err(error);
        }
    };
    let delivered = deliver(
        &database,
        &result.receipt,
        result.callback.as_ref(),
        text_output,
    )
    .await;
    let closed = database.close().await;
    delivered?;
    closed
}

fn select_capsule(
    request: &mut TaskRequest,
    capsule_id: &str,
) -> Result<crate::capsule::CapsuleAdvertisement> {
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
    request.engine = capsule.engine.clone();
    request.capsule = Some(crate::capsule::select(capsule_id)?);
    Ok(capsule.clone())
}

async fn submit_detached(request: TaskRequest, socket: Option<PathBuf>) -> Result<()> {
    let socket = match socket {
        Some(path) => path,
        None => crate::control::default_socket_path()?,
    };
    let handle = crate::control::submit(socket, request)
        .await
        .map_err(|error| {
            Error::new(
                error.kind(),
                format!("{error}; start the service with 'spewer serve --engine codex'"),
            )
        })?;
    let task_id = &handle.task_id;
    println!(
        "{}",
        serde_json::to_string_pretty(&json!({
            "handle": handle,
            "next": {
                "watch": ["spewer", "watch", task_id],
                "observe": ["spewer", "observe", task_id, "--after", "0"],
                "result": ["spewer", "result", task_id],
                "cancel": ["spewer", "cancel", task_id]
            }
        }))?
    );
    Ok(())
}

async fn wait_with_progress<F>(future: F, database: &Database, task_id: &str) -> Result<RunResult>
where
    F: Future<Output = Result<RunResult>>,
{
    if !std::io::stderr().is_terminal() {
        return future.await;
    }
    let mut future = Box::pin(future);
    let started = Instant::now();
    let mut interval = tokio::time::interval(Duration::from_millis(250));
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    let mut tick = 0_u64;
    loop {
        tokio::select! {
            result = &mut future => {
                clear_progress()?;
                return result;
            }
            _instant = interval.tick() => {
                show_progress(database, task_id, tick, started.elapsed().as_secs()).await?;
                tick = tick.saturating_add(1);
            }
        }
    }
}

async fn show_progress(
    database: &Database,
    task_id: &str,
    tick: u64,
    elapsed_seconds: u64,
) -> Result<()> {
    let spinner = match tick % 4 {
        0 => '◐',
        1 => '◓',
        2 => '◑',
        _ => '◒',
    };
    let detail = match database.get(task_id.to_owned()).await {
        Ok(Some(projection)) => format!(
            "{:?}/{:?} · input={} · tools={}",
            projection.status,
            projection.phase,
            optional(projection.usage.input_tokens),
            projection.usage.tool_calls
        )
        .to_ascii_lowercase(),
        Ok(None) | Err(_) => "accepting".to_owned(),
    };
    eprint!("\r\x1b[2K{spinner} spewer {detail} · {elapsed_seconds}s");
    std::io::stderr().flush()?;
    Ok(())
}

fn clear_progress() -> Result<()> {
    eprint!("\r\x1b[2K");
    std::io::stderr().flush()?;
    Ok(())
}

async fn deliver(
    database: &Database,
    receipt: &Receipt,
    callback: Option<&crate::delivery::OutboxMessage>,
    text_output: bool,
) -> Result<()> {
    if text_output {
        let answer = receipt.summary.trim();
        if answer.is_empty() {
            println!("No answer was returned.");
        } else {
            println!("{answer}");
        }
        eprintln!("{}", telemetry(receipt)?);
    } else {
        println!(
            "{}",
            serde_json::to_string_pretty(&json!({
                "answer": receipt.summary,
                "task_id": receipt.task_id,
                "status": receipt.status,
                "receipt": receipt
            }))?
        );
    }
    std::io::stdout().flush()?;
    let callback = callback.ok_or_else(|| {
        Error::new(
            ErrorKind::Storage,
            "ask completed without a durable callback",
        )
    })?;
    let applied = database
        .acknowledge(callback.message_id.clone(), "spewer-ask".to_owned())
        .await?;
    if !applied {
        return Err(Error::new(
            ErrorKind::Storage,
            "ask callback was already acknowledged",
        ));
    }
    Ok(())
}

fn confirm_overwrite(path: &std::path::Path) -> Result<bool> {
    eprint!("Overwrite {}? [Y/n] ", path.display());
    std::io::stderr().flush()?;
    let mut response = String::new();
    std::io::stdin().lock().read_line(&mut response)?;
    match response.trim().to_ascii_lowercase().as_str() {
        "" | "y" | "yes" => Ok(true),
        "n" | "no" => Ok(false),
        _ => Err(Error::new(
            ErrorKind::InvalidInput,
            "overwrite response must be yes or no",
        )),
    }
}

fn telemetry(receipt: &Receipt) -> Result<String> {
    let status = serde_json::to_string(&receipt.status)?;
    let status = status.trim_matches('"');
    let usage = &receipt.usage;
    let cost = display_cost(usage.actual_cost_usd, &receipt.engine.kind);
    Ok(format!(
        "spewer: status={status} model={} input={} cached={} output={} reasoning={} tools={} wall_ms={} cost={} task={}",
        receipt.engine.requested_model,
        optional(usage.input_tokens),
        optional(usage.cached_input_tokens),
        optional(usage.output_tokens),
        optional(usage.reasoning_tokens),
        usage.tool_calls,
        usage.wall_ms,
        cost,
        receipt.task_id
    ))
}

fn optional(value: Option<u64>) -> String {
    value.map_or_else(|| "not-reported".to_owned(), |number| number.to_string())
}

fn display_cost(value: Option<f64>, engine: &str) -> String {
    value.map_or_else(
        || match engine {
            crate::ollama::ENGINE_KIND => "local-unpriced".to_owned(),
            _ => "not-reported".to_owned(),
        },
        |cost| format!("${cost:.6}"),
    )
}

#[cfg(test)]
mod display_tests {
    use super::{display_cost, optional};

    #[test]
    fn missing_local_telemetry_has_explicit_labels() {
        assert_eq!(optional(None), "not-reported");
        assert_eq!(display_cost(None, "ollama"), "local-unpriced");
        assert_eq!(display_cost(None, "codex-app-server"), "not-reported");
    }
}

#[cfg(test)]
mod authority_tests {
    use super::grant_danger_full_access;
    use crate::protocol::TaskRequest;

    #[test]
    fn danger_authority_is_codex_only() -> Result<(), Box<dyn std::error::Error>> {
        let fixture = include_str!("../../tests/fixtures/task-request.json");
        let mut codex: TaskRequest = serde_json::from_str(fixture)?;
        grant_danger_full_access(&mut codex, "codex-app-server")?;
        assert_eq!(codex.permissions.filesystem, "danger-full-access");
        assert_eq!(codex.permissions.network, "allow");
        assert!(
            codex
                .context
                .notes
                .last()
                .is_some_and(|note| { note.contains("explicitly granted") })
        );

        let mut ollama: TaskRequest = serde_json::from_str(fixture)?;
        assert!(grant_danger_full_access(&mut ollama, "ollama").is_err());
        assert_eq!(ollama.permissions.filesystem, "workspace-write");
        Ok(())
    }
}
