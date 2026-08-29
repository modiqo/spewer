//! Ollama discovery and bounded local inference.

mod http;
mod prompt;
mod search;
mod tool;

use crate::engine::{EngineAdapter, EngineCapabilities, EngineEvent, negotiate};
use crate::error::{Error, ErrorKind, Result};
use crate::protocol::TaskRequest;
use crate::util::sha256;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::time::Duration;
use tool::{
    MAX_WEB_SEARCH_CALLS, ToolCall, ToolExecution, WEB_SEARCH_TOOL, parse_web_search_call,
    web_search_tool,
};

/// Engine discriminator stored in capsule manifests and task requests.
pub const ENGINE_KIND: &str = "ollama";
/// Qwen3 model used by the CP18 reference capsule.
pub const DEFAULT_QWEN_MODEL: &str = "qwen3:30b-a3b";

/// Whether this process can authenticate Ollama hosted web search.
pub(crate) fn web_search_configured() -> bool {
    search::is_configured()
}

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
    search: Option<search::SearchClient>,
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
        let search =
            search::SearchClient::from_env(Duration::from_secs(request.budgets.wall_seconds))?;
        let mut models = report.models;
        if !models.iter().any(|model| model == &request.engine.model) {
            models.push(request.engine.model.clone());
            models.sort();
        }
        Ok(Self {
            client,
            search,
            capabilities: EngineCapabilities {
                kind: ENGINE_KIND.to_owned(),
                models,
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

    async fn chat(&self, request: &TaskRequest) -> Result<ConversationResponse> {
        let num_predict = request.budgets.tokens.min(32_768);
        let web_enabled = request.permissions.network == "allow";
        let search = if web_enabled {
            Some(self.search.as_ref().ok_or_else(|| {
                Error::new(
                    ErrorKind::InvalidInput,
                    "web_search requires OLLAMA_API_KEY in the Spewer process environment",
                )
            })?)
        } else {
            None
        };
        let tools = web_enabled.then(web_search_tool);
        let mut messages = vec![json!({"role": "user", "content": self.prompt})];
        let mut executions = Vec::new();
        let mut prompt_eval_count = None;
        let mut eval_count = None;
        let max_calls = request.budgets.tool_calls.min(MAX_WEB_SEARCH_CALLS);
        loop {
            let mut body = json!({
                "model": request.engine.model,
                "messages": messages,
                "stream": false,
                "think": false,
                "keep_alive": "5m",
                "options": {"num_predict": num_predict}
            });
            if let Some(tool) = &tools {
                body.as_object_mut()
                    .ok_or_else(|| {
                        Error::new(ErrorKind::EngineProtocol, "Ollama request is not an object")
                    })?
                    .insert("tools".to_owned(), json!([tool]));
            }
            let value = self.client.post_json("/api/chat", &body).await?;
            let response: ChatResponse = serde_json::from_value(value)?;
            validate_chat_response(&response, &request.engine.model)?;
            add_optional_count(&mut prompt_eval_count, response.prompt_eval_count)?;
            add_optional_count(&mut eval_count, response.eval_count)?;
            if response.message.tool_calls.is_empty() {
                if visible_answer(&response.message.content).is_empty() {
                    return Err(Error::new(
                        ErrorKind::EngineProtocol,
                        "Ollama returned an empty answer",
                    ));
                }
                return Ok(ConversationResponse {
                    model: response.model,
                    message: response.message,
                    done_reason: response.done_reason,
                    prompt_eval_count,
                    eval_count,
                    executions,
                });
            }
            let search = search.ok_or_else(|| {
                Error::new(
                    ErrorKind::EngineProtocol,
                    "Ollama requested a tool for a network-denied task",
                )
            })?;
            messages.push(serde_json::to_value(&response.message)?);
            for call in response.message.tool_calls {
                let used = u64::try_from(executions.len())
                    .map_err(|error| Error::new(ErrorKind::InvalidInput, error.to_string()))?;
                let (call_id, arguments) = parse_web_search_call(call, used, max_calls)?;
                let result = search.search(&arguments.query).await?;
                let content = serde_json::to_string(&result)?;
                let mut tool_message = json!({
                    "role": "tool",
                    "tool_name": WEB_SEARCH_TOOL,
                    "content": content
                });
                if let Some(id) = call_id {
                    tool_message
                        .as_object_mut()
                        .ok_or_else(|| {
                            Error::new(
                                ErrorKind::EngineProtocol,
                                "Ollama tool result is not an object",
                            )
                        })?
                        .insert("tool_call_id".to_owned(), Value::String(id));
                }
                messages.push(tool_message);
                executions.push(ToolExecution {
                    query: arguments.query,
                    result_count: result.results.len(),
                });
            }
        }
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
    let resolved_model = required_model
        .map(|required| {
            resolve_installed_model(&models, required).ok_or_else(|| {
                Error::new(
                    ErrorKind::InvalidInput,
                    format!(
                        "Ollama model {required} is not installed; run 'ollama pull {required}'"
                    ),
                )
            })
        })
        .transpose()?;
    Ok(OllamaDoctorReport {
        ready: true,
        version: version.version,
        models,
        required_model: resolved_model,
    })
}

fn resolve_installed_model(models: &[String], required: &str) -> Option<String> {
    models
        .iter()
        .find(|model| model.as_str() == required)
        .cloned()
        .or_else(|| {
            let name = required.rsplit('/').next()?;
            if name.contains(':') {
                return None;
            }
            let latest = format!("{required}:latest");
            models.iter().find(|model| **model == latest).cloned()
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

fn events(response: &ConversationResponse) -> Result<Vec<EngineEvent>> {
    let digest = sha256(&serde_json::to_vec(response)?)?;
    let answer = visible_answer(&response.message.content);
    let event = |method: &str, kind: &str, data: Value, ordinal: u64| EngineEvent {
        method: method.to_owned(),
        kind: kind.to_owned(),
        data,
        source_key: format!("ollama:{digest}:{ordinal}"),
    };
    let mut output = vec![
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
    ];
    let mut ordinal = 3_u64;
    for execution in &response.executions {
        output.push(event(
            "tool/started",
            "item.started",
            json!({
                "item": {
                    "type": "tool_call",
                    "name": WEB_SEARCH_TOOL,
                    "arguments": {"query": execution.query}
                },
                "tool": true
            }),
            ordinal,
        ));
        ordinal = ordinal.saturating_add(1);
        output.push(event(
            "tool/completed",
            "item.completed",
            json!({
                "tool": WEB_SEARCH_TOOL,
                "result_count": execution.result_count
            }),
            ordinal,
        ));
        ordinal = ordinal.saturating_add(1);
    }
    output.push(event(
        "message/started",
        "item.started",
        json!({"item":{"type":"agent_message"},"tool":false}),
        ordinal,
    ));
    ordinal = ordinal.saturating_add(1);
    output.push(event(
        "message/completed",
        "item.completed",
        json!({"summary":answer}),
        ordinal,
    ));
    ordinal = ordinal.saturating_add(1);
    output.push(event(
        "usage",
        "usage.updated",
        json!({
            "input_tokens": response.prompt_eval_count,
            "output_tokens": response.eval_count
        }),
        ordinal,
    ));
    ordinal = ordinal.saturating_add(1);
    output.push(event(
        "chat/completed",
        "turn.completed",
        json!({"status":"completed","done_reason":response.done_reason}),
        ordinal,
    ));
    Ok(output)
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
    #[serde(default = "assistant_role")]
    role: String,
    #[serde(default)]
    content: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    tool_calls: Vec<ToolCall>,
}

#[derive(Debug, Serialize)]
struct ConversationResponse {
    model: String,
    message: ChatMessage,
    done_reason: Option<String>,
    prompt_eval_count: Option<u64>,
    eval_count: Option<u64>,
    executions: Vec<ToolExecution>,
}

fn assistant_role() -> String {
    "assistant".to_owned()
}

fn validate_chat_response(response: &ChatResponse, requested_model: &str) -> Result<()> {
    if !response.done {
        return Err(Error::new(
            ErrorKind::EngineProtocol,
            "Ollama chat ended without done=true",
        ));
    }
    if response.model != requested_model {
        return Err(Error::new(
            ErrorKind::EngineProtocol,
            "Ollama responded with a different model",
        ));
    }
    Ok(())
}

fn add_optional_count(total: &mut Option<u64>, next: Option<u64>) -> Result<()> {
    let Some(next) = next else {
        return Ok(());
    };
    *total = Some(match *total {
        Some(current) => current
            .checked_add(next)
            .ok_or_else(|| Error::new(ErrorKind::EngineProtocol, "Ollama usage overflow"))?,
        None => next,
    });
    Ok(())
}

#[cfg(test)]
mod tests;
