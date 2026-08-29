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

pub(super) async fn ask(
    question: String,
    workspace: Option<PathBuf>,
    capsule_id: Option<String>,
    text_output: bool,
    detach: bool,
    socket: Option<PathBuf>,
) -> Result<()> {
    let config = tokio::task::spawn_blocking(crate::config::LocalConfig::load).await??;
    let mut request = config.infer_question(&question, workspace)?;
    if let Some(capsule_id) = capsule_id {
        select_capsule(&mut request, &capsule_id)?;
    }
    if detach {
        "poll".clone_into(&mut request.callback.mode);
        return submit_detached(request, socket).await;
    }
    Box::pin(run_attached(request, text_output)).await
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

fn select_capsule(request: &mut TaskRequest, capsule_id: &str) -> Result<()> {
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
    request.validate()?;
    Ok(())
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
    let cost = usage
        .actual_cost_usd
        .map_or_else(|| "unknown".to_owned(), |value| format!("${value:.6}"));
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
    value.map_or_else(|| "unknown".to_owned(), |number| number.to_string())
}
