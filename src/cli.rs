//! Machine-readable command-line interface with lifecycle-directed help.

mod help;
mod parse;
mod question;
mod service;
mod setup;
mod skill_install;

use crate::codex::{CodexConfig, doctor};
use crate::error::Result;
use crate::protocol::TaskRequest;
use crate::runner::run_codex_durable;
use crate::store::Database;
use parse::{CliCommand, HelpTopic, parse};
use serde_json::json;
use std::path::PathBuf;

/// Parses process arguments, runs one command, and writes JSON to stdout.
pub async fn run() -> Result<()> {
    match parse(std::env::args_os().skip(1))? {
        CliCommand::Install {
            workspace,
            max_workers,
            skip_codex_install,
        } => setup::install(workspace, max_workers, skip_codex_install).await?,
        CliCommand::CapsuleList => setup::capsule_list()?,
        CliCommand::CapsuleAdd {
            capsule_id,
            engine,
            model,
        } => setup::capsule_add(&capsule_id, &engine, &model).await?,
        CliCommand::CapsuleBind { capsule_id, skill } => {
            setup::capsule_bind(&capsule_id, &skill)?;
        }
        CliCommand::CapsuleUnbind(capsule_id) => setup::capsule_unbind(&capsule_id)?,
        CliCommand::Init {
            workspace,
            overwrite,
        } => question::initialize(workspace, overwrite).await?,
        CliCommand::Ask {
            question: prompt,
            workspace,
            capsule_id,
            text,
            detach,
            socket,
        } => {
            Box::pin(question::ask(
                prompt, workspace, capsule_id, text, detach, socket,
            ))
            .await?;
        }
        CliCommand::DoctorCodex => {
            let report = doctor(CodexConfig::default()).await?;
            let json = serde_json::to_string_pretty(&report)?;
            println!("{json}");
        }
        CliCommand::DoctorOllama { model } => {
            let report =
                crate::ollama::doctor(crate::ollama::OllamaConfig::default(), model.as_deref())
                    .await?;
            println!("{}", serde_json::to_string_pretty(&report)?);
        }
        CliCommand::RunCodex(path) => run_task(path).await?,
        CliCommand::RunOllama(path) => run_ollama_task(path).await?,
        CliCommand::Serve {
            max_workers,
            socket,
            detach,
        } => service::serve(max_workers, socket, detach).await?,
        CliCommand::Submit { path, socket } => submit(path, socket).await?,
        CliCommand::Delegate {
            path,
            capsule_id,
            socket,
        } => delegate(path, capsule_id, socket).await?,
        CliCommand::Check {
            task_id,
            after,
            socket,
        } => check(task_id, after, socket).await?,
        CliCommand::Load { socket } => load(socket).await?,
        CliCommand::Stop { socket } => stop(socket).await?,
        CliCommand::Capabilities { socket } => capabilities(socket).await?,
        CliCommand::Observe {
            task_id,
            after,
            socket,
        } => observe(task_id, after, socket).await?,
        CliCommand::Result { task_id, socket } => result(task_id, socket).await?,
        CliCommand::Cancel {
            task_id,
            reason,
            socket,
        } => cancel(task_id, reason, socket).await?,
        CliCommand::Status(task_id) => show_status(task_id).await?,
        CliCommand::Tail { task_id, after } => tail(task_id, after).await?,
        CliCommand::Rebuild(task_id) => rebuild(task_id).await?,
        CliCommand::Resume(task_id) => resume(task_id).await?,
        CliCommand::Recover => recover().await?,
        CliCommand::Outbox(consumer_id) => outbox(consumer_id).await?,
        CliCommand::Acknowledge {
            message_id,
            consumer_id,
            socket,
        } => acknowledge(message_id, consumer_id, socket).await?,
        CliCommand::Help(topic) => print_help(topic),
        CliCommand::Version => println!("spewer {}", env!("CARGO_PKG_VERSION")),
    }
    Ok(())
}

async fn run_task(path: PathBuf) -> Result<()> {
    let request = read_request(path).await?;
    let database = Database::open(Database::default_path()?).await?;
    let outcome = run_codex_durable(request, CodexConfig::default(), &database).await;
    let close = database.close().await;
    let result = outcome?;
    close?;
    println!(
        "{}",
        serde_json::to_string(&json!({"handle": result.handle}))?
    );
    for event in result.events {
        println!("{}", serde_json::to_string(&json!({"event": event}))?);
    }
    println!(
        "{}",
        serde_json::to_string(&json!({"receipt": result.receipt}))?
    );
    if let Some(callback) = result.callback {
        println!("{}", serde_json::to_string(&json!({"callback": callback}))?);
    }
    Ok(())
}

async fn run_ollama_task(path: PathBuf) -> Result<()> {
    let request = read_request(path).await?;
    let database = Database::open(Database::default_path()?).await?;
    let outcome = crate::runner::run_ollama_durable(
        request,
        crate::ollama::OllamaConfig::default(),
        &database,
    )
    .await;
    let close = database.close().await;
    let result = outcome?;
    close?;
    println!(
        "{}",
        serde_json::to_string(&json!({"handle": result.handle}))?
    );
    for event in result.events {
        println!("{}", serde_json::to_string(&json!({"event": event}))?);
    }
    println!(
        "{}",
        serde_json::to_string(&json!({"receipt": result.receipt}))?
    );
    if let Some(callback) = result.callback {
        println!("{}", serde_json::to_string(&json!({"callback": callback}))?);
    }
    Ok(())
}

async fn submit(path: PathBuf, socket: Option<PathBuf>) -> Result<()> {
    let request = read_request(path).await?;
    let handle = crate::control::submit(socket_path(socket)?, request).await?;
    println!("{}", serde_json::to_string_pretty(&handle)?);
    Ok(())
}

async fn delegate(path: PathBuf, capsule_id: String, socket: Option<PathBuf>) -> Result<()> {
    let request = read_request(path).await?;
    let client = crate::harness::HarnessClient::new(socket_path(socket)?);
    let delegation = client.delegate(request, &capsule_id).await?;
    println!("{}", serde_json::to_string_pretty(&delegation)?);
    Ok(())
}

async fn check(task_id: String, after: u64, socket: Option<PathBuf>) -> Result<()> {
    let client = crate::harness::HarnessClient::new(socket_path(socket)?);
    let check = client.check(task_id, after).await?;
    println!("{}", serde_json::to_string_pretty(&check)?);
    Ok(())
}

async fn load(socket: Option<PathBuf>) -> Result<()> {
    let load = crate::control::load(socket_path(socket)?).await?;
    println!("{}", serde_json::to_string_pretty(&load)?);
    Ok(())
}

async fn stop(socket: Option<PathBuf>) -> Result<()> {
    let response = crate::control::stop(socket_path(socket)?).await?;
    println!("{}", serde_json::to_string_pretty(&response)?);
    Ok(())
}

async fn capabilities(socket: Option<PathBuf>) -> Result<()> {
    let capabilities = crate::control::capabilities(socket_path(socket)?).await?;
    println!("{}", serde_json::to_string_pretty(&capabilities)?);
    Ok(())
}

async fn observe(task_id: String, after: u64, socket: Option<PathBuf>) -> Result<()> {
    let observation = crate::control::observe(socket_path(socket)?, task_id, after).await?;
    println!("{}", serde_json::to_string_pretty(&observation)?);
    Ok(())
}

async fn result(task_id: String, socket: Option<PathBuf>) -> Result<()> {
    let result = crate::control::result(socket_path(socket)?, task_id).await?;
    println!(
        "{}",
        serde_json::to_string_pretty(&json!({
            "ready": result.message.is_some(),
            "result": result
        }))?
    );
    Ok(())
}

async fn cancel(task_id: String, reason: String, socket: Option<PathBuf>) -> Result<()> {
    let cancellation = crate::control::cancel(socket_path(socket)?, task_id, reason).await?;
    println!("{}", serde_json::to_string_pretty(&cancellation)?);
    Ok(())
}

async fn read_request(path: PathBuf) -> Result<TaskRequest> {
    let task_json = tokio::task::spawn_blocking(move || std::fs::read_to_string(path)).await??;
    Ok(serde_json::from_str(&task_json)?)
}

pub(super) fn socket_path(path: Option<PathBuf>) -> Result<PathBuf> {
    match path {
        Some(path) => Ok(path),
        None => crate::control::default_socket_path(),
    }
}

async fn resume(task_id: String) -> Result<()> {
    let database = Database::open(Database::default_path()?).await?;
    let report = crate::recovery::resume_codex(&database, task_id, CodexConfig::default()).await;
    let close = database.close().await;
    let result = report?;
    println!("{}", serde_json::to_string_pretty(&result.receipt)?);
    close?;
    Ok(())
}

async fn recover() -> Result<()> {
    let database = Database::open(Database::default_path()?).await?;
    let tasks = database.nonterminal().await?;
    database.close().await?;
    println!("{}", serde_json::to_string_pretty(&tasks)?);
    Ok(())
}

async fn outbox(consumer_id: String) -> Result<()> {
    let database = Database::open(Database::default_path()?).await?;
    let messages = database.pending(consumer_id).await?;
    database.close().await?;
    for message in messages {
        println!("{}", serde_json::to_string(&message)?);
    }
    Ok(())
}

async fn acknowledge(
    message_id: String,
    consumer_id: String,
    socket: Option<PathBuf>,
) -> Result<()> {
    let applied =
        crate::control::acknowledge(socket_path(socket)?, message_id, consumer_id).await?;
    println!("{}", serde_json::to_string(&json!({"applied": applied}))?);
    Ok(())
}

async fn show_status(task_id: String) -> Result<()> {
    let database = Database::open(Database::default_path()?).await?;
    let projection = database.get(task_id).await?;
    database.close().await?;
    println!("{}", serde_json::to_string_pretty(&projection)?);
    Ok(())
}

async fn tail(task_id: String, after: u64) -> Result<()> {
    let database = Database::open(Database::default_path()?).await?;
    let events = database.events_after(task_id, after).await?;
    database.close().await?;
    for event in events {
        println!("{}", serde_json::to_string(&event)?);
    }
    Ok(())
}

async fn rebuild(task_id: String) -> Result<()> {
    let database = Database::open(Database::default_path()?).await?;
    let projection = database.rebuild(task_id).await?;
    database.close().await?;
    println!("{}", serde_json::to_string_pretty(&projection)?);
    Ok(())
}

fn print_help(topic: Option<HelpTopic>) {
    print!("{}", help::render(topic));
}
