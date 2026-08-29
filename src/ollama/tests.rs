use super::search::SearchClient;
use super::tool::{ToolCall, ToolFunction, parse_web_search_call};
use super::{ENGINE_KIND, OllamaConfig, OllamaEngine, doctor, validate_request, visible_answer};
use crate::engine::EngineAdapter;
use crate::error::ErrorKind;
use crate::protocol::TaskRequest;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

#[test]
fn adapter_accepts_only_read_only_ollama_tasks() -> Result<(), Box<dyn std::error::Error>> {
    let mut request: TaskRequest =
        serde_json::from_str(include_str!("../../tests/fixtures/task-request.json"))?;
    request.engine.kind = ENGINE_KIND.to_owned();
    request.engine.model = "qwen3:30b-a3b".to_owned();
    request.permissions.filesystem = "read-only".to_owned();
    request.permissions.writable_paths.clear();
    validate_request(&request)?;
    request.permissions.filesystem = "workspace-write".to_owned();
    assert!(validate_request(&request).is_err());
    Ok(())
}

#[test]
fn reasoning_tags_do_not_leak_into_the_receipt_summary() {
    assert_eq!(
        visible_answer("<think>private reasoning</think>\nFinal answer"),
        "Final answer"
    );
    assert_eq!(visible_answer("Direct answer"), "Direct answer");
}

#[test]
fn malformed_and_excess_search_calls_are_rejected() {
    let malformed = ToolCall {
        id: None,
        function: ToolFunction {
            name: "web_search".to_owned(),
            arguments: serde_json::json!({"not_query": "value"}),
        },
    };
    let error = parse_web_search_call(malformed, 0, 1).err();
    assert!(error.is_some_and(|error| {
        error.kind() == ErrorKind::EngineProtocol
            && error.to_string().contains("invalid web_search arguments")
    }));
    let excess = ToolCall {
        id: None,
        function: ToolFunction {
            name: "web_search".to_owned(),
            arguments: serde_json::json!({"query": "current information"}),
        },
    };
    let error = parse_web_search_call(excess, 1, 1).err();
    assert!(error.is_some_and(|error| {
        error.kind() == ErrorKind::EngineProtocol && error.to_string().contains("tool-call budget")
    }));
}

#[tokio::test(flavor = "current_thread")]
async fn adapter_discovers_and_normalizes_one_chat() -> Result<(), Box<dyn std::error::Error>> {
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let address = listener.local_addr()?;
    let server = tokio::spawn(async move { serve_fixture(listener).await });
    let nanos = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
    let workspace =
        std::env::temp_dir().join(format!("spewer-ollama-unit-{}-{nanos}", std::process::id()));
    std::fs::create_dir(&workspace)?;
    let mut request: TaskRequest =
        serde_json::from_str(include_str!("../../tests/fixtures/task-request.json"))?;
    request.engine.kind = ENGINE_KIND.to_owned();
    request.engine.model = "qwen3:30b-a3b".to_owned();
    request.permissions.filesystem = "read-only".to_owned();
    request.permissions.writable_paths.clear();
    request.context.files.clear();
    let config = OllamaConfig {
        endpoint: format!("http://{address}"),
        startup_timeout: Duration::from_secs(2),
    };
    let mut engine = OllamaEngine::connect(config, &request, &workspace).await?;
    assert_eq!(engine.capabilities().models, ["qwen3:30b-a3b"]);
    let events = engine.execute(&request).await?;
    assert_eq!(events.len(), 6);
    assert_eq!(
        events.get(3).map(|event| event.kind.as_str()),
        Some("item.completed")
    );
    assert_eq!(
        events
            .get(3)
            .and_then(|event| event.data.get("summary"))
            .and_then(serde_json::Value::as_str),
        Some("fixture answer")
    );
    let requests = server.await??;
    assert_eq!(requests.len(), 3);
    assert!(
        requests
            .get(2)
            .is_some_and(|body| body.contains("/no_think"))
    );
    std::fs::remove_dir_all(workspace)?;
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn adapter_executes_one_bounded_web_search() -> Result<(), Box<dyn std::error::Error>> {
    let ollama_listener = TcpListener::bind("127.0.0.1:0").await?;
    let ollama_address = ollama_listener.local_addr()?;
    let ollama_server = tokio::spawn(async move {
        serve_responses(
            ollama_listener,
            vec![
                r#"{"version":"test-1"}"#,
                r#"{"models":[{"name":"qwen3:30b-a3b"}]}"#,
                r#"{"model":"qwen3:30b-a3b","message":{"role":"assistant","content":"","tool_calls":[{"id":"call-one","function":{"name":"web_search","arguments":{"query":"current Sunnyvale weather"}}}]},"done":true,"done_reason":"stop","prompt_eval_count":10,"eval_count":2}"#,
                r#"{"model":"qwen3:30b-a3b","message":{"role":"assistant","content":"Sunnyvale is sunny. Source: https://weather.example/sunnyvale"},"done":true,"done_reason":"stop","prompt_eval_count":20,"eval_count":4}"#,
            ],
        )
        .await
    });
    let search_listener = TcpListener::bind("127.0.0.1:0").await?;
    let search_address = search_listener.local_addr()?;
    let search_server = tokio::spawn(async move {
        serve_responses(
            search_listener,
            vec![
                r#"{"results":[{"title":"Sunnyvale weather","url":"https://weather.example/sunnyvale","content":"Sunny, 72 F"}]}"#,
            ],
        )
        .await
    });
    let workspace = temporary_workspace("search")?;
    let mut request = ollama_request(&workspace)?;
    request.permissions.network = "allow".to_owned();
    let config = OllamaConfig {
        endpoint: format!("http://{ollama_address}"),
        startup_timeout: Duration::from_secs(2),
    };
    let mut engine = OllamaEngine::connect(config, &request, &workspace).await?;
    engine.search = Some(SearchClient::new(
        format!("http://{search_address}"),
        "fixture-search-key".to_owned(),
        Duration::from_secs(2),
    )?);
    let events = engine.execute(&request).await?;
    assert_eq!(events.len(), 8);
    assert!(events.iter().any(|event| {
        event.kind == "item.started"
            && event.data.get("tool").and_then(serde_json::Value::as_bool) == Some(true)
    }));
    let usage = events
        .iter()
        .find(|event| event.kind == "usage.updated")
        .ok_or("usage event missing")?;
    assert_eq!(
        usage
            .data
            .get("input_tokens")
            .and_then(serde_json::Value::as_u64),
        Some(30)
    );
    assert_eq!(
        usage
            .data
            .get("output_tokens")
            .and_then(serde_json::Value::as_u64),
        Some(6)
    );
    let serialized = serde_json::to_string(&events)?;
    assert!(!serialized.contains("fixture-search-key"));
    let ollama_requests = ollama_server.await??;
    assert_eq!(ollama_requests.len(), 4);
    assert!(
        ollama_requests
            .get(2)
            .is_some_and(|body| body.contains("web_search"))
    );
    assert!(
        ollama_requests
            .get(3)
            .is_some_and(|body| body.contains("Sunny, 72 F"))
    );
    let search_requests = search_server.await??;
    assert_eq!(search_requests.len(), 1);
    assert!(search_requests.first().is_some_and(|request| {
        request
            .to_ascii_lowercase()
            .contains("authorization: bearer fixture-search-key")
            && request.contains("current Sunnyvale weather")
    }));
    std::fs::remove_dir_all(workspace)?;
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn web_search_requires_a_configured_key() -> Result<(), Box<dyn std::error::Error>> {
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let address = listener.local_addr()?;
    let server = tokio::spawn(async move {
        serve_responses(
            listener,
            vec![
                r#"{"version":"test-1"}"#,
                r#"{"models":[{"name":"qwen3:30b-a3b"}]}"#,
            ],
        )
        .await
    });
    let workspace = temporary_workspace("missing-search-key")?;
    let mut request = ollama_request(&workspace)?;
    request.permissions.network = "allow".to_owned();
    let config = OllamaConfig {
        endpoint: format!("http://{address}"),
        startup_timeout: Duration::from_secs(2),
    };
    let mut engine = OllamaEngine::connect(config, &request, &workspace).await?;
    engine.search = None;
    let error = engine
        .execute(&request)
        .await
        .err()
        .ok_or("missing error")?;
    assert_eq!(error.kind(), ErrorKind::InvalidInput);
    assert!(error.to_string().contains("OLLAMA_API_KEY"));
    assert_eq!(server.await??.len(), 2);
    std::fs::remove_dir_all(workspace)?;
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn adapter_rejects_an_unknown_model_tool() -> Result<(), Box<dyn std::error::Error>> {
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let address = listener.local_addr()?;
    let server = tokio::spawn(async move {
        serve_responses(
            listener,
            vec![
                r#"{"version":"test-1"}"#,
                r#"{"models":[{"name":"qwen3:30b-a3b"}]}"#,
                r#"{"model":"qwen3:30b-a3b","message":{"role":"assistant","content":"","tool_calls":[{"function":{"name":"shell","arguments":{"command":"env"}}}]},"done":true}"#,
            ],
        )
        .await
    });
    let workspace = temporary_workspace("unknown-tool")?;
    let mut request = ollama_request(&workspace)?;
    request.permissions.network = "allow".to_owned();
    let config = OllamaConfig {
        endpoint: format!("http://{address}"),
        startup_timeout: Duration::from_secs(2),
    };
    let mut engine = OllamaEngine::connect(config, &request, &workspace).await?;
    engine.search = Some(SearchClient::new(
        "http://127.0.0.1:9",
        "fixture-search-key".to_owned(),
        Duration::from_secs(1),
    )?);
    let error = engine
        .execute(&request)
        .await
        .err()
        .ok_or("missing error")?;
    assert_eq!(error.kind(), ErrorKind::EngineProtocol);
    assert!(error.to_string().contains("unknown tool shell"));
    assert_eq!(server.await??.len(), 3);
    std::fs::remove_dir_all(workspace)?;
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn doctor_rejects_a_missing_model() -> Result<(), Box<dyn std::error::Error>> {
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let address = listener.local_addr()?;
    let server = tokio::spawn(async move {
        serve_responses(
            listener,
            vec![
                r#"{"version":"test-1"}"#,
                r#"{"models":[{"name":"qwen3:30b-a3b"}]}"#,
            ],
        )
        .await
    });
    let config = OllamaConfig {
        endpoint: format!("http://{address}"),
        startup_timeout: Duration::from_secs(2),
    };
    let result = doctor(config, Some("missing-model")).await;
    let Err(error) = result else {
        return Err("doctor accepted a missing model".into());
    };
    assert_eq!(error.kind(), ErrorKind::InvalidInput);
    assert_eq!(server.await??.len(), 2);
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn doctor_resolves_an_untagged_model_to_latest() -> Result<(), Box<dyn std::error::Error>> {
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let address = listener.local_addr()?;
    let server = tokio::spawn(async move {
        serve_responses(
            listener,
            vec![
                r#"{"version":"test-1"}"#,
                r#"{"models":[{"name":"mistral:latest"}]}"#,
            ],
        )
        .await
    });
    let config = OllamaConfig {
        endpoint: format!("http://{address}"),
        startup_timeout: Duration::from_secs(2),
    };
    let report = doctor(config, Some("mistral")).await?;
    assert_eq!(report.required_model.as_deref(), Some("mistral:latest"));
    assert_eq!(server.await??.len(), 2);
    Ok(())
}

async fn serve_fixture(listener: TcpListener) -> Result<Vec<String>, std::io::Error> {
    serve_responses(
        listener,
        vec![
        r#"{"version":"test-1"}"#,
        r#"{"models":[{"name":"qwen3:30b-a3b"},{"name":"remote:cloud"}]}"#,
        r#"{"model":"qwen3:30b-a3b","message":{"content":"<think>hidden</think>fixture answer"},"done":true,"done_reason":"stop","prompt_eval_count":12,"eval_count":3}"#,
        ],
    )
    .await
}

fn temporary_workspace(name: &str) -> Result<std::path::PathBuf, Box<dyn std::error::Error>> {
    let nanos = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
    let workspace = std::env::temp_dir().join(format!(
        "spewer-ollama-{name}-{}-{nanos}",
        std::process::id()
    ));
    std::fs::create_dir(&workspace)?;
    Ok(workspace)
}

fn ollama_request(workspace: &std::path::Path) -> Result<TaskRequest, Box<dyn std::error::Error>> {
    let mut request: TaskRequest =
        serde_json::from_str(include_str!("../../tests/fixtures/task-request.json"))?;
    request.engine.kind = ENGINE_KIND.to_owned();
    request.engine.model = "qwen3:30b-a3b".to_owned();
    request.workspace.path = workspace.to_string_lossy().into_owned();
    request.permissions.filesystem = "read-only".to_owned();
    request.permissions.writable_paths.clear();
    request.context.files.clear();
    Ok(request)
}

async fn serve_responses(
    listener: TcpListener,
    responses: Vec<&'static str>,
) -> Result<Vec<String>, std::io::Error> {
    let mut requests = Vec::new();
    for response in responses {
        let (mut stream, _peer) = listener.accept().await?;
        let request = read_request(&mut stream).await?;
        requests.push(request);
        let reply = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{response}",
            response.len()
        );
        stream.write_all(reply.as_bytes()).await?;
    }
    Ok(requests)
}

async fn read_request(stream: &mut tokio::net::TcpStream) -> Result<String, std::io::Error> {
    let mut bytes = Vec::new();
    let mut buffer = [0_u8; 4096];
    loop {
        let count = stream.read(&mut buffer).await?;
        if count == 0 {
            break;
        }
        let read = match buffer.get(..count) {
            Some(read) => read,
            None => &[],
        };
        bytes.extend_from_slice(read);
        let Some(header_end) = bytes.windows(4).position(|window| window == b"\r\n\r\n") else {
            continue;
        };
        let header_bytes = match bytes.get(..header_end) {
            Some(headers) => headers,
            None => &[],
        };
        let headers = String::from_utf8_lossy(header_bytes);
        let parsed_length = headers.lines().find_map(|line| {
            line.to_ascii_lowercase()
                .strip_prefix("content-length:")
                .and_then(|value| value.trim().parse::<usize>().ok())
        });
        let content_length = parsed_length.map_or(0, |length| length);
        let expected = header_end.saturating_add(4).saturating_add(content_length);
        if bytes.len() >= expected {
            break;
        }
    }
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}
