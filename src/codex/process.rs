use super::wire::{ParsedLine, parse_line, write_message};
use crate::error::{Error, ErrorKind, Result};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::ffi::OsString;
use std::path::PathBuf;
use std::process::Stdio;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::{Child, ChildStderr, ChildStdin, ChildStdout, Command};
use tokio::sync::{mpsc, oneshot};
use tokio::task::JoinHandle;

const COMMAND_CAPACITY: usize = 32;
const EVENT_CAPACITY: usize = 256;

/// App Server executable, environment, and deadline configuration.
#[derive(Clone, Debug)]
pub struct CodexConfig {
    /// Codex executable path.
    pub program: PathBuf,
    /// Arguments that start App Server.
    pub app_server_args: Vec<OsString>,
    /// Parent environment variables inherited by the child.
    pub inherited_environment: Vec<String>,
    /// Deadline for initialization.
    pub startup_timeout: Duration,
    /// Deadline for one JSON-RPC request.
    pub request_timeout: Duration,
    /// Deadline for graceful process termination.
    pub shutdown_timeout: Duration,
}

impl Default for CodexConfig {
    fn default() -> Self {
        Self {
            program: std::env::var_os("SPEWER_CODEX_BIN")
                .map_or_else(|| PathBuf::from("codex"), PathBuf::from),
            app_server_args: vec![OsString::from("app-server"), OsString::from("--stdio")],
            inherited_environment: vec![
                "HOME".to_owned(),
                "PATH".to_owned(),
                "USER".to_owned(),
                "TMPDIR".to_owned(),
                "CODEX_HOME".to_owned(),
            ],
            startup_timeout: Duration::from_secs(15),
            request_timeout: Duration::from_secs(30),
            shutdown_timeout: Duration::from_secs(5),
        }
    }
}

/// One message observed outside request-response correlation.
#[derive(Clone, Debug)]
pub enum CodexMessage {
    /// App Server notification.
    Notification {
        /// Native method name.
        method: String,
        /// Native parameters.
        params: Value,
    },
    /// Request initiated by App Server, such as an approval.
    ServerRequest {
        /// Native JSON-RPC identifier.
        id: Value,
        /// Native method name.
        method: String,
        /// Native parameters.
        params: Value,
    },
    /// A malformed stdout line that did not stop the reader.
    Malformed {
        /// Original line.
        line: String,
        /// Parse failure.
        error: String,
    },
    /// One stderr line retained for diagnostics.
    Stderr(String),
    /// The child closed stdout and exited.
    Exited(Option<i32>),
}

enum DriverCommand {
    Request {
        id: u64,
        method: String,
        params: Value,
        reply: oneshot::Sender<Result<Value>>,
    },
    Notify {
        method: String,
        params: Value,
        reply: oneshot::Sender<Result<()>>,
    },
    Respond {
        id: Value,
        result: Value,
        reply: oneshot::Sender<Result<()>>,
    },
    Shutdown {
        reply: oneshot::Sender<Result<()>>,
    },
}

/// An initialized, exclusively owned App Server connection.
#[derive(Debug)]
pub struct CodexClient {
    commands: mpsc::Sender<DriverCommand>,
    events: mpsc::Receiver<CodexMessage>,
    driver: Option<JoinHandle<Result<()>>>,
    process_group: Option<u32>,
    next_request_id: u64,
    request_timeout: Duration,
    shutdown_timeout: Duration,
    initialization: Value,
}

impl Drop for CodexClient {
    fn drop(&mut self) {
        if let Some(driver) = self.driver.take() {
            terminate_abandoned_process_group(self.process_group.take());
            driver.abort();
        }
    }
}

impl CodexClient {
    /// Starts App Server and completes the required initialize handshake.
    pub async fn connect(config: CodexConfig) -> Result<Self> {
        let mut client = Self::spawn_uninitialized(&config)?;
        client.initialize(config.startup_timeout).await?;
        Ok(client)
    }

    /// Starts App Server without sending data, allowing durable process registration first.
    pub(crate) fn spawn_uninitialized(config: &CodexConfig) -> Result<Self> {
        let request_timeout = config.request_timeout;
        let shutdown_timeout = config.shutdown_timeout;
        let (child, stdin, stdout, stderr) = spawn_child(config)?;
        let process_group = child.id();
        let (command_tx, command_rx) = mpsc::channel(COMMAND_CAPACITY);
        let (event_tx, event_rx) = mpsc::channel(EVENT_CAPACITY);
        let driver = tokio::spawn(run_driver(
            child,
            stdin,
            stdout,
            stderr,
            command_rx,
            event_tx,
            shutdown_timeout,
        ));
        Ok(Self {
            commands: command_tx,
            events: event_rx,
            driver: Some(driver),
            process_group,
            next_request_id: 1,
            request_timeout,
            shutdown_timeout,
            initialization: Value::Null,
        })
    }

    /// Returns the process group that must be bound to the active task lease.
    pub(crate) const fn process_group(&self) -> Option<u32> {
        self.process_group
    }

    /// Completes the App Server handshake after process custody is durable.
    pub(crate) async fn initialize(&mut self, startup_timeout: Duration) -> Result<()> {
        let initialize = tokio::time::timeout(
            startup_timeout,
            self.request(
                "initialize",
                json!({
                    "clientInfo": {
                        "name": "spewer",
                        "title": "Spewer",
                        "version": env!("CARGO_PKG_VERSION")
                    }
                }),
            ),
        )
        .await
        .map_err(|_| Error::new(ErrorKind::Timeout, "App Server initialization timed out"))??;
        self.notify("initialized", json!({}))
            .await
            .map_err(|error| Error::new(error.kind(), format!("initialized: {error}")))?;
        self.initialization = initialize;
        Ok(())
    }

    /// Returns the immutable initialization response.
    pub const fn initialization(&self) -> &Value {
        &self.initialization
    }

    /// Sends one correlated JSON-RPC request.
    pub async fn request(&mut self, method: &str, params: Value) -> Result<Value> {
        let id = self.next_request_id;
        self.next_request_id = self
            .next_request_id
            .checked_add(1)
            .ok_or_else(|| Error::new(ErrorKind::EngineProtocol, "request id exhausted"))?;
        let (reply_tx, reply_rx) = oneshot::channel();
        self.commands
            .send(DriverCommand::Request {
                id,
                method: method.to_owned(),
                params,
                reply: reply_tx,
            })
            .await
            .map_err(|_| Error::new(ErrorKind::ChannelClosed, "App Server driver closed"))?;
        tokio::time::timeout(self.request_timeout, reply_rx)
            .await
            .map_err(|_| Error::new(ErrorKind::Timeout, format!("{method} timed out")))?
            .map_err(|_| Error::new(ErrorKind::ChannelClosed, "request reply closed"))?
    }

    /// Sends one JSON-RPC notification.
    pub async fn notify(&self, method: &str, params: Value) -> Result<()> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.commands
            .send(DriverCommand::Notify {
                method: method.to_owned(),
                params,
                reply: reply_tx,
            })
            .await
            .map_err(|_| Error::new(ErrorKind::ChannelClosed, "App Server driver closed"))?;
        reply_rx
            .await
            .map_err(|_| Error::new(ErrorKind::ChannelClosed, "notification reply closed"))?
    }

    /// Responds to an App Server-initiated request.
    pub async fn respond(&self, id: Value, result: Value) -> Result<()> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.commands
            .send(DriverCommand::Respond {
                id,
                result,
                reply: reply_tx,
            })
            .await
            .map_err(|_| Error::new(ErrorKind::ChannelClosed, "App Server driver closed"))?;
        reply_rx
            .await
            .map_err(|_| Error::new(ErrorKind::ChannelClosed, "response reply closed"))?
    }

    /// Waits for the next notification, request, diagnostic, or exit.
    pub async fn next_message(&mut self) -> Option<CodexMessage> {
        self.events.recv().await
    }

    /// Terminates App Server and waits for the driver task.
    pub async fn close(mut self) -> Result<()> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.commands
            .send(DriverCommand::Shutdown { reply: reply_tx })
            .await
            .map_err(|_| Error::new(ErrorKind::ChannelClosed, "App Server driver closed"))?;
        tokio::time::timeout(self.shutdown_timeout, reply_rx)
            .await
            .map_err(|_| Error::new(ErrorKind::Timeout, "App Server shutdown timed out"))?
            .map_err(|_| Error::new(ErrorKind::ChannelClosed, "shutdown reply closed"))??;
        if let Some(driver) = self.driver.take() {
            driver.await??;
        }
        self.process_group = None;
        Ok(())
    }
}

fn spawn_child(config: &CodexConfig) -> Result<(Child, ChildStdin, ChildStdout, ChildStderr)> {
    let mut command = Command::new(&config.program);
    command
        .args(&config.app_server_args)
        .env_clear()
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    for name in &config.inherited_environment {
        if let Some(value) = std::env::var_os(name) {
            command.env(name, value);
        }
    }
    #[cfg(unix)]
    {
        command.process_group(0);
    }
    let mut child = command.spawn()?;
    let stdin = child
        .stdin
        .take()
        .ok_or_else(|| Error::new(ErrorKind::Io, "child stdin unavailable"))?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| Error::new(ErrorKind::Io, "child stdout unavailable"))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| Error::new(ErrorKind::Io, "child stderr unavailable"))?;
    Ok((child, stdin, stdout, stderr))
}

async fn run_driver(
    mut child: Child,
    mut stdin: ChildStdin,
    stdout: ChildStdout,
    stderr: ChildStderr,
    mut commands: mpsc::Receiver<DriverCommand>,
    events: mpsc::Sender<CodexMessage>,
    shutdown_timeout: Duration,
) -> Result<()> {
    let mut stdout = BufReader::new(stdout).lines();
    let mut stderr = BufReader::new(stderr).lines();
    let mut pending = HashMap::<String, oneshot::Sender<Result<Value>>>::new();
    loop {
        tokio::select! {
            command = commands.recv() => {
                let Some(command) = command else { break; };
                if handle_command(command, &mut stdin, &mut pending, &mut child, shutdown_timeout).await? {
                    return Ok(());
                }
            }
            line = stdout.next_line() => {
                match line? {
                    Some(line) => handle_line(line, &events, &mut pending).await?,
                    None => break,
                }
            }
            line = stderr.next_line() => {
                if let Some(line) = line? {
                    let _sent = events.send(CodexMessage::Stderr(line)).await;
                }
            }
        }
    }
    fail_pending(&mut pending, "App Server exited before replying");
    let status = child.wait().await?;
    let _sent = events.send(CodexMessage::Exited(status.code())).await;
    Ok(())
}

async fn handle_command(
    command: DriverCommand,
    stdin: &mut ChildStdin,
    pending: &mut HashMap<String, oneshot::Sender<Result<Value>>>,
    child: &mut Child,
    shutdown_timeout: Duration,
) -> Result<bool> {
    match command {
        DriverCommand::Request {
            id,
            method,
            params,
            reply,
        } => {
            let key = id.to_string();
            pending.insert(key.clone(), reply);
            if let Err(error) = write_message(
                stdin,
                &json!({"method": method, "id": id, "params": params}),
            )
            .await
                && let Some(reply) = pending.remove(&key)
            {
                let _sent = reply.send(Err(error));
            }
            Ok(false)
        }
        DriverCommand::Notify {
            method,
            params,
            reply,
        } => {
            let result = write_message(stdin, &json!({"method": method, "params": params})).await;
            let _sent = reply.send(result);
            Ok(false)
        }
        DriverCommand::Respond { id, result, reply } => {
            let result = write_message(stdin, &json!({"id": id, "result": result})).await;
            let _sent = reply.send(result);
            Ok(false)
        }
        DriverCommand::Shutdown { reply } => {
            fail_pending(pending, "App Server stopped before replying");
            let result = shutdown_child(child, shutdown_timeout).await;
            let success = result.is_ok();
            let _sent = reply.send(result);
            Ok(success)
        }
    }
}

async fn handle_line(
    line: String,
    events: &mpsc::Sender<CodexMessage>,
    pending: &mut HashMap<String, oneshot::Sender<Result<Value>>>,
) -> Result<()> {
    match parse_line(&line) {
        ParsedLine::Response { id, result } => {
            if let Some(reply) = pending.remove(&id) {
                let _sent = reply.send(result);
            } else {
                let _sent = events
                    .send(CodexMessage::Malformed {
                        line,
                        error: format!("response id {id} is not pending"),
                    })
                    .await;
            }
        }
        ParsedLine::Notification { method, params } => {
            let _sent = events
                .send(CodexMessage::Notification { method, params })
                .await;
        }
        ParsedLine::ServerRequest { id, method, params } => {
            let _sent = events
                .send(CodexMessage::ServerRequest { id, method, params })
                .await;
        }
        ParsedLine::Malformed(error) => {
            let _sent = events.send(CodexMessage::Malformed { line, error }).await;
        }
    }
    Ok(())
}

fn fail_pending(pending: &mut HashMap<String, oneshot::Sender<Result<Value>>>, message: &str) {
    for (_, reply) in pending.drain() {
        let _sent = reply.send(Err(Error::new(ErrorKind::EngineProtocol, message)));
    }
}

async fn shutdown_child(child: &mut Child, deadline: Duration) -> Result<()> {
    if child.try_wait()?.is_some() {
        return Ok(());
    }
    terminate_process_group(child)?;
    if tokio::time::timeout(deadline, child.wait()).await.is_ok() {
        return Ok(());
    }
    child.kill().await?;
    let _status = child.wait().await?;
    Ok(())
}

#[cfg(unix)]
fn terminate_process_group(child: &Child) -> Result<()> {
    use nix::sys::signal::{Signal, killpg};
    use nix::unistd::Pid;

    let Some(id) = child.id() else {
        return Ok(());
    };
    let pid = i32::try_from(id).map_err(|error| Error::new(ErrorKind::Io, error.to_string()))?;
    killpg(Pid::from_raw(pid), Signal::SIGTERM)
        .map_err(|error| Error::new(ErrorKind::Io, error.to_string()))
}

#[cfg(unix)]
fn terminate_abandoned_process_group(id: Option<u32>) {
    use nix::sys::signal::{Signal, killpg};
    use nix::unistd::Pid;

    if let Some(id) = id.and_then(|value| i32::try_from(value).ok()) {
        let _ignored = killpg(Pid::from_raw(id), Signal::SIGTERM);
    }
}

#[cfg(not(unix))]
fn terminate_abandoned_process_group(_id: Option<u32>) {}

#[cfg(not(unix))]
fn terminate_process_group(child: &mut Child) -> Result<()> {
    child.start_kill()?;
    Ok(())
}
