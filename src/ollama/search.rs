use crate::error::{Error, ErrorKind, Result};
use serde::{Deserialize, Serialize};
use std::fmt;
use std::time::Duration;

const API_KEY_ENV: &str = "OLLAMA_API_KEY";
const SEARCH_ENDPOINT: &str = "https://ollama.com/api/web_search";
const MAX_QUERY_BYTES: usize = 2_048;
const MAX_RESPONSE_BYTES: usize = 1_048_576;
const MAX_RESULTS: usize = 5;
const MAX_RESULT_CONTENT_BYTES: usize = 8_192;

#[derive(Clone)]
pub(super) struct SearchClient {
    client: ureq::Agent,
    endpoint: String,
    api_key: String,
}

impl fmt::Debug for SearchClient {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SearchClient")
            .field("endpoint", &self.endpoint)
            .field("api_key", &"[redacted]")
            .finish_non_exhaustive()
    }
}

impl SearchClient {
    pub(super) fn from_env(timeout: Duration) -> Result<Option<Self>> {
        let Some(api_key) = configured_key() else {
            return Ok(None);
        };
        Self::new(SEARCH_ENDPOINT, api_key, timeout).map(Some)
    }

    pub(super) fn new(
        endpoint: impl Into<String>,
        api_key: String,
        timeout: Duration,
    ) -> Result<Self> {
        if api_key.trim().is_empty() {
            return Err(Error::new(
                ErrorKind::InvalidInput,
                "Ollama search requires a nonempty OLLAMA_API_KEY",
            ));
        }
        let client: ureq::Agent = ureq::Agent::config_builder()
            .timeout_global(Some(timeout))
            .http_status_as_error(false)
            .max_redirects(0)
            .build()
            .into();
        Ok(Self {
            client,
            endpoint: endpoint.into(),
            api_key,
        })
    }

    pub(super) async fn search(&self, query: &str) -> Result<SearchResponse> {
        let query = query.trim();
        if query.is_empty() || query.len() > MAX_QUERY_BYTES {
            return Err(Error::new(
                ErrorKind::InvalidInput,
                "web_search query must contain 1-2048 bytes",
            ));
        }
        let client = self.client.clone();
        let endpoint = self.endpoint.clone();
        let api_key = self.api_key.clone();
        let query = query.to_owned();
        tokio::task::spawn_blocking(move || search_blocking(&client, &endpoint, &api_key, &query))
            .await?
    }
}

fn search_blocking(
    client: &ureq::Agent,
    endpoint: &str,
    api_key: &str,
    query: &str,
) -> Result<SearchResponse> {
    let request = SearchRequest {
        query,
        max_results: MAX_RESULTS,
    };
    let authorization = format!("Bearer {api_key}");
    let mut response = client
        .post(endpoint)
        .header("Authorization", &authorization)
        .send_json(&request)
        .map_err(|error| search_protocol_error(&error))?;
    let status = response.status();
    let bytes = response
        .body_mut()
        .with_config()
        .limit(
            u64::try_from(MAX_RESPONSE_BYTES)
                .map_err(|error| Error::new(ErrorKind::InvalidInput, error.to_string()))?,
        )
        .read_to_vec()
        .map_err(|error| search_protocol_error(&error))?;
    if !status.is_success() {
        return Err(search_error(format!(
            "Ollama search HTTP {}",
            status.as_u16()
        )));
    }
    let mut parsed: SearchResponse = serde_json::from_slice(&bytes)?;
    parsed.results.truncate(MAX_RESULTS);
    for result in &mut parsed.results {
        truncate_utf8(&mut result.content, MAX_RESULT_CONTENT_BYTES);
    }
    Ok(parsed)
}

pub(super) fn is_configured() -> bool {
    configured_key().is_some()
}

fn configured_key() -> Option<String> {
    std::env::var(API_KEY_ENV)
        .ok()
        .filter(|value| !value.trim().is_empty())
}

fn truncate_utf8(value: &mut String, limit: usize) {
    if value.len() <= limit {
        return;
    }
    let mut boundary = limit;
    while boundary > 0 && !value.is_char_boundary(boundary) {
        boundary = boundary.saturating_sub(1);
    }
    value.truncate(boundary);
}

fn search_protocol_error(error: &ureq::Error) -> Error {
    search_error(error.to_string())
}

fn search_error(message: impl Into<String>) -> Error {
    Error::new(ErrorKind::EngineProtocol, message)
}

#[derive(Debug, Serialize)]
struct SearchRequest<'a> {
    query: &'a str,
    max_results: usize,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(super) struct SearchResponse {
    pub(super) results: Vec<SearchResult>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(super) struct SearchResult {
    #[serde(default)]
    pub(super) title: String,
    #[serde(default)]
    pub(super) url: String,
    #[serde(default)]
    pub(super) content: String,
}
