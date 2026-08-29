//! Ollama discovery and bounded local inference.

mod http;
mod prompt;

use crate::engine::{EngineAdapter, EngineCapabilities, EngineEvent, negotiate};
use crate::error::{Error, ErrorKind, Result};
use crate::protocol::TaskRequest;
use crate::util::sha256;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::time::Duration;

/// Engine discriminator stored in capsule manifests and task requests.
pub const ENGINE_KIND: &str = "ollama";
/// Qwen3 model used by the CP18 reference capsule.
pub const DEFAULT_QWEN_MODEL: &str = "qwen3:30b-a3b";

/// Connection settings for one local Ollama server.
#[derive(Clone, Debug)]
pub struct OllamaConfig {
    /// HTTP endpoint. CP18 accepts loopback HTTP only.
    pub endpoint: String,
    /// Maximum time for discovery requests.
    pub startup_timeout: Duration,
}

impl Default for OllamaConfig {
    fn default() -> Self {
        let endpoint = match std::env::var("OLLAMA_HOST") {
            Ok(endpoint) => endpoint,
            Err(_) => "http://127.0.0.1:11434".to_owned(),
        };
        Self {
            endpoint,
            startup_timeout: Duration::from_secs(5),
        }
    }
}

/// Readiness and model evidence returned before task execution.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct OllamaDoctorReport {
    /// Whether the server answered both discovery requests.
    pub ready: bool,
    /// Ollama server version.
    pub version: String,
    /// Installed local model names.
    pub models: Vec<String>,
    /// Required model when the caller requested one.
    pub required_model: Option<String>,
}

/// Production adapter for a local Ollama inference server.
#[derive(Clone, Debug)]
pub struct OllamaEngine {
    client: http::HttpClient,
    capabilities: EngineCapabilities,
    version: String,
    prompt: String,
}

impl OllamaEngine {
    /// Connects, discovers installed models, and prepares projected task context.
    pub async fn connect(
        config: OllamaConfig,
        request: &TaskRequest,
        workspace: &std::path::Path,
    ) -> Result<Self> {
        validate_request(request)?;
        let discovery = http::HttpClient::new(&config.endpoint, config.startup_timeout)?;
        let report = discover(&discovery, Some(&request.engine.model)).await?;
        let client = http::HttpClient::new(
            &config.endpoint,
            Duration::from_secs(request.budgets.wall_seconds),
        )?;
        let prompt = prompt::build(request, workspace).await?;
        Ok(Self {
            client,
            capabilities: EngineCapabilities {
                kind: ENGINE_KIND.to_owned(),
                models: report.models,
                resumable: false,
                usage: true,
            },
            version: report.version,
            prompt,
        })
    }

    /// Returns the discovered Ollama server version.
    pub fn version(&self) -> &str {
        &self.version
    }

    async fn chat(&self, request: &TaskRequest) -> Result<ChatResponse> {
        let num_predict = request.budgets.tokens.min(32_768);
        let body = json!({
            "model": request.engine.model,
            "messages": [{"role": "user", "content": self.prompt}],
            "stream": false,
            "think": false,
            "keep_alive": "5m",
            "options": {"num_predict": num_predict}
        });
        let value = self.client.post_json("/api/chat", &body).await?;
        let response: ChatResponse = serde_json::from_value(value)?;
        if !response.done {
            return Err(Error::new(
                ErrorKind::EngineProtocol,
                "Ollama chat ended without done=true",
            ));
        }
        if response.model != request.engine.model {
            return Err(Error::new(
                ErrorKind::EngineProtocol,
                "Ollama responded with a different model",
            ));
        }
        if visible_answer(&response.message.content).is_empty() {
            return Err(Error::new(
                ErrorKind::EngineProtocol,
                "Ollama returned an empty answer",
            ));
        }
        Ok(response)
    }
}

impl EngineAdapter for OllamaEngine {
    fn capabilities(&self) -> &EngineCapabilities {
        &self.capabilities
    }

    async fn execute(&mut self, request: &TaskRequest) -> Result<Vec<EngineEvent>> {
        negotiate(&self.capabilities, request, false)?;
        validate_request(request)?;
        let response = self.chat(request).await?;
        events(&response)
    }
}

/// Checks the local server and optionally requires one installed model.
pub async fn doctor(
    config: OllamaConfig,
    required_model: Option<&str>,
) -> Result<OllamaDoctorReport> {
    let client = http::HttpClient::new(&config.endpoint, config.startup_timeout)?;
    discover(&client, required_model).await
}

async fn discover(
    client: &http::HttpClient,
    required_model: Option<&str>,
) -> Result<OllamaDoctorReport> {
    let version: VersionResponse = serde_json::from_value(client.get_json("/api/version").await?)?;
    let tags: TagsResponse = serde_json::from_value(client.get_json("/api/tags").await?)?;
    let mut models = tags
        .models
        .into_iter()
        .map(|model| model.name)
        .filter(|name| !name.ends_with(":cloud"))
        .collect::<Vec<_>>();
    models.sort();
    models.dedup();
    if let Some(required) = required_model
        && !models.iter().any(|model| model == required)
    {
        return Err(Error::new(
            ErrorKind::InvalidInput,
            format!("Ollama model {required} is not installed; run 'ollama pull {required}'"),
        ));
    }
    Ok(OllamaDoctorReport {
        ready: true,
        version: version.version,
        models,
        required_model: required_model.map(str::to_owned),
    })
}

fn validate_request(request: &TaskRequest) -> Result<()> {
    if request.engine.kind != ENGINE_KIND {
        return Err(Error::new(
            ErrorKind::InvalidInput,
            "Ollama adapter requires engine.kind=ollama",
        ));
    }
    if request.permissions.filesystem != "read-only"
        || !request.permissions.writable_paths.is_empty()
    {
        return Err(Error::new(
            ErrorKind::InvalidInput,
            "the Ollama inference adapter accepts read-only tasks only",
        ));
    }
    if request.permissions.commands == "allowlist" {
        return Err(Error::new(
            ErrorKind::InvalidInput,
            "the Ollama inference adapter does not execute commands",
        ));
    }
    Ok(())
}

fn events(response: &ChatResponse) -> Result<Vec<EngineEvent>> {
    let digest = sha256(&serde_json::to_vec(response)?)?;
    let answer = visible_answer(&response.message.content);
    let event = |method: &str, kind: &str, data: Value, ordinal: u8| EngineEvent {
        method: method.to_owned(),
        kind: kind.to_owned(),
        data,
        source_key: format!("ollama:{digest}:{ordinal}"),
    };
    Ok(vec![
        event(
            "chat/accepted",
            "engine.bound",
            json!({"thread_id": digest, "session_id": null}),
            1,
        ),
        event(
            "chat/started",
            "turn.started",
            json!({"turn_id": digest}),
            2,
        ),
        event(
            "message/started",
            "item.started",
            json!({"item":{"type":"agent_message"},"tool":false}),
            3,
        ),
        event(
            "message/completed",
            "item.completed",
            json!({"summary":answer}),
            4,
        ),
        event(
            "usage",
            "usage.updated",
            json!({
                "input_tokens": response.prompt_eval_count,
                "output_tokens": response.eval_count
            }),
            5,
        ),
        event(
            "chat/completed",
            "turn.completed",
            json!({"status":"completed","done_reason":response.done_reason}),
            6,
        ),
    ])
}

fn visible_answer(content: &str) -> String {
    content
        .rsplit_once("</think>")
        .map_or(content, |(_thinking, answer)| answer)
        .trim()
        .to_owned()
}

#[derive(Debug, Deserialize)]
struct VersionResponse {
    version: String,
}

#[derive(Debug, Deserialize)]
struct TagsResponse {
    models: Vec<TagModel>,
}

#[derive(Debug, Deserialize)]
struct TagModel {
    name: String,
}

#[derive(Debug, Deserialize, Serialize)]
struct ChatResponse {
    model: String,
    message: ChatMessage,
    done: bool,
    #[serde(default)]
    done_reason: Option<String>,
    #[serde(default)]
    prompt_eval_count: Option<u64>,
    #[serde(default)]
    eval_count: Option<u64>,
}

#[derive(Debug, Deserialize, Serialize)]
struct ChatMessage {
    content: String,
}

#[cfg(test)]
mod tests;
