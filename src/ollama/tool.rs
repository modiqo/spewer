use crate::error::{Error, ErrorKind, Result};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

pub(super) const WEB_SEARCH_TOOL: &str = "web_search";
pub(super) const MAX_WEB_SEARCH_CALLS: u64 = 8;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(super) struct ToolCall {
    #[serde(default)]
    pub(super) id: Option<String>,
    pub(super) function: ToolFunction,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(super) struct ToolFunction {
    pub(super) name: String,
    pub(super) arguments: Value,
}

#[derive(Debug, Deserialize)]
pub(super) struct WebSearchArguments {
    pub(super) query: String,
}

#[derive(Debug, Serialize)]
pub(super) struct ToolExecution {
    pub(super) query: String,
    pub(super) result_count: usize,
}

pub(super) fn web_search_tool() -> Value {
    json!({
        "type": "function",
        "function": {
            "name": WEB_SEARCH_TOOL,
            "description": "Search the public web for current information. Returns titles, URLs, and relevant text snippets.",
            "parameters": {
                "type": "object",
                "required": ["query"],
                "properties": {
                    "query": {
                        "type": "string",
                        "description": "A focused web search query"
                    }
                }
            }
        }
    })
}

pub(super) fn parse_web_search_call(
    call: ToolCall,
    used: u64,
    max_calls: u64,
) -> Result<(Option<String>, WebSearchArguments)> {
    if used >= max_calls {
        return Err(Error::new(
            ErrorKind::EngineProtocol,
            "Ollama exceeded the web_search tool-call budget",
        ));
    }
    if call.function.name != WEB_SEARCH_TOOL {
        return Err(Error::new(
            ErrorKind::EngineProtocol,
            format!("Ollama requested unknown tool {}", call.function.name),
        ));
    }
    let arguments = serde_json::from_value(call.function.arguments).map_err(|error| {
        Error::new(
            ErrorKind::EngineProtocol,
            format!("invalid web_search arguments: {error}"),
        )
    })?;
    Ok((call.id, arguments))
}
