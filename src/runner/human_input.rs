//! Same-turn continuation across one typed human-input boundary.

use crate::codex::CodexClient;
use crate::error::{Error, ErrorKind, Result};
use crate::journal::TaskJournal;
use crate::protocol::TaskInputResponse;
use crate::util::now;
use serde_json::{Value, json};
use std::time::Duration;
use tokio::sync::mpsc;
use tokio::time::Instant;

pub(crate) struct DriveOptions<'a> {
    pub(crate) deadline: Instant,
    pub(crate) input: Option<&'a mut mpsc::Receiver<TaskInputResponse>>,
}

pub(super) async fn await_response(
    client: &mut CodexClient,
    task: &mut TaskJournal<'_>,
    receiver: &mut mpsc::Receiver<TaskInputResponse>,
    request_id: Value,
    method: &str,
) -> Result<Option<Duration>> {
    let waiting = Instant::now();
    let Some(response) = next_input(
        receiver,
        Duration::from_secs(crate::protocol::HUMAN_INPUT_TIMEOUT_SECONDS),
    )
    .await?
    else {
        append_timeout(task).await?;
        return Ok(None);
    };
    if response.request_id != request_id {
        return Err(Error::new(
            ErrorKind::InvalidInput,
            "parent input response changed request identity",
        ));
    }
    if let Some(database) = task.database {
        task.projection = database
            .get(task.projection.task_id.clone())
            .await?
            .ok_or_else(|| Error::new(ErrorKind::Storage, "input task disappeared"))?;
    }
    validate_codex_response(method, &response.response)?;
    client.respond(request_id, response.response).await?;
    Ok(Some(waiting.elapsed()))
}

async fn append_timeout(task: &mut TaskJournal<'_>) -> Result<()> {
    task.append(
        "task.stalled",
        json!({"reason":"parent input was not received within 30 minutes"}),
        None,
        None,
        now()?,
    )
    .await?;
    task.append(
        "task.escalated",
        json!({"reason":"human input timed out after 30 minutes"}),
        None,
        None,
        now()?,
    )
    .await?;
    Ok(())
}

async fn next_input(
    receiver: &mut mpsc::Receiver<TaskInputResponse>,
    timeout: Duration,
) -> Result<Option<TaskInputResponse>> {
    match tokio::time::timeout(timeout, receiver.recv()).await {
        Ok(Some(response)) => Ok(Some(response)),
        Ok(None) => Err(Error::new(
            ErrorKind::ChannelClosed,
            "parent input channel closed",
        )),
        Err(_) => Ok(None),
    }
}

fn validate_codex_response(method: &str, response: &Value) -> Result<()> {
    if !response.is_object() {
        return Err(Error::new(
            ErrorKind::InvalidInput,
            "Codex input response must be a JSON object",
        ));
    }
    match method {
        "item/tool/requestUserInput"
        | "mcpServer/elicitation/request"
        | "item/commandExecution/requestApproval"
        | "item/fileChange/requestApproval" => Ok(()),
        _ => Err(Error::new(
            ErrorKind::InvalidInput,
            format!("unsupported Codex input method {method}"),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::next_input;
    use crate::error::Result;
    use crate::protocol::TaskInputResponse;
    use serde_json::json;
    use std::time::Duration;
    use tokio::sync::mpsc;

    #[tokio::test]
    async fn wait_times_out_without_guessing() -> Result<()> {
        let (_sender, mut receiver) = mpsc::channel(1);
        let response = next_input(&mut receiver, Duration::from_millis(1)).await?;
        assert!(response.is_none());
        Ok(())
    }

    #[tokio::test]
    async fn wait_returns_the_supplied_response() -> Result<()> {
        let (sender, mut receiver) = mpsc::channel(1);
        let supplied = TaskInputResponse {
            request_id: json!(99),
            response: json!({"answers":{"dates":{"answers":["August 1–15"]}}}),
        };
        assert!(sender.send(supplied.clone()).await.is_ok());
        let response = next_input(&mut receiver, Duration::from_secs(1)).await?;
        assert_eq!(response, Some(supplied));
        Ok(())
    }
}
