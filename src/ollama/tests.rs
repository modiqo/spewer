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
