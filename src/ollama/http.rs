use crate::error::{Error, ErrorKind, Result};
use serde_json::Value;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

const MAX_RESPONSE_BYTES: u64 = 8 * 1_048_576;

#[derive(Clone, Debug)]
pub(super) struct HttpClient {
    host: String,
    port: u16,
    timeout: Duration,
}

impl HttpClient {
    pub(super) fn new(endpoint: &str, timeout: Duration) -> Result<Self> {
        let authority = endpoint.strip_prefix("http://").ok_or_else(|| {
            Error::new(
                ErrorKind::InvalidInput,
                "OLLAMA_HOST must use loopback HTTP",
            )
        })?;
        if authority.contains('/') {
            return Err(Error::new(
                ErrorKind::InvalidInput,
                "OLLAMA_HOST must not contain a path",
            ));
        }
        let (host, port) = match authority.rsplit_once(':') {
            Some((host, port)) => {
                let port = port.parse::<u16>().map_err(|error| {
                    Error::new(
                        ErrorKind::InvalidInput,
                        format!("invalid Ollama port: {error}"),
                    )
                })?;
                (host, port)
            }
            None => (authority, 11434),
        };
        if !matches!(host, "127.0.0.1" | "localhost") {
            return Err(Error::new(
                ErrorKind::InvalidInput,
                "CP18 restricts Ollama to 127.0.0.1 or localhost",
            ));
        }
        Ok(Self {
            host: host.to_owned(),
            port,
            timeout,
        })
    }

    pub(super) async fn get_json(&self, path: &str) -> Result<Value> {
        self.request("GET", path, None).await
    }

    pub(super) async fn post_json(&self, path: &str, body: &Value) -> Result<Value> {
        self.request("POST", path, Some(body)).await
    }

    async fn request(&self, method: &str, path: &str, body: Option<&Value>) -> Result<Value> {
        let operation = async {
            let mut stream = TcpStream::connect((self.host.as_str(), self.port)).await?;
            let encoded = body
                .map(serde_json::to_vec)
                .transpose()?
                .into_iter()
                .flatten()
                .collect::<Vec<_>>();
            let request = format!(
                "{method} {path} HTTP/1.1\r\nHost: {}:{}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                self.host,
                self.port,
                encoded.len()
            );
            stream.write_all(request.as_bytes()).await?;
            stream.write_all(&encoded).await?;
            stream.flush().await?;
            let mut response = Vec::new();
            stream
                .take(MAX_RESPONSE_BYTES.saturating_add(1))
                .read_to_end(&mut response)
                .await?;
            if u64::try_from(response.len()).map_or(true, |size| size > MAX_RESPONSE_BYTES) {
                return Err(Error::new(
                    ErrorKind::EngineProtocol,
                    "Ollama response exceeded 8 MiB",
                ));
            }
            parse_response(&response)
        };
        tokio::time::timeout(self.timeout, operation)
            .await
            .map_err(|_| Error::new(ErrorKind::Timeout, "Ollama HTTP request timed out"))?
    }
}

fn parse_response(response: &[u8]) -> Result<Value> {
    let split = response
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .ok_or_else(|| Error::new(ErrorKind::EngineProtocol, "Ollama returned no HTTP headers"))?;
    let body_start = split
        .checked_add(4)
        .ok_or_else(|| Error::new(ErrorKind::EngineProtocol, "HTTP header overflow"))?;
    let headers =
        std::str::from_utf8(response.get(..split).ok_or_else(|| {
            Error::new(ErrorKind::EngineProtocol, "invalid HTTP header boundary")
        })?)
        .map_err(|error| Error::new(ErrorKind::EngineProtocol, error.to_string()))?;
    let status = headers
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .and_then(|value| value.parse::<u16>().ok())
        .ok_or_else(|| Error::new(ErrorKind::EngineProtocol, "invalid Ollama HTTP status"))?;
    let raw_body = response.get(body_start..).ok_or_else(|| {
        Error::new(
            ErrorKind::EngineProtocol,
            "invalid Ollama HTTP body boundary",
        )
    })?;
    let body = if headers.lines().any(|line| {
        line.to_ascii_lowercase()
            .starts_with("transfer-encoding: chunked")
    }) {
        decode_chunked(raw_body)?
    } else {
        raw_body.to_vec()
    };
    if !(200..300).contains(&status) {
        let detail = String::from_utf8_lossy(&body);
        return Err(Error::new(
            ErrorKind::EngineProtocol,
            format!(
                "Ollama HTTP {status}: {}",
                detail.chars().take(512).collect::<String>()
            ),
        ));
    }
    Ok(serde_json::from_slice(&body)?)
}

fn decode_chunked(mut input: &[u8]) -> Result<Vec<u8>> {
    let mut output = Vec::new();
    loop {
        let end = input
            .windows(2)
            .position(|window| window == b"\r\n")
            .ok_or_else(|| Error::new(ErrorKind::EngineProtocol, "invalid chunk header"))?;
        let size_text =
            std::str::from_utf8(input.get(..end).ok_or_else(|| {
                Error::new(ErrorKind::EngineProtocol, "invalid chunk size boundary")
            })?)
            .map_err(|error| Error::new(ErrorKind::EngineProtocol, error.to_string()))?;
        let size_part = size_text.split(';').next().map_or("", |value| value);
        let size = usize::from_str_radix(size_part, 16)
            .map_err(|error| Error::new(ErrorKind::EngineProtocol, error.to_string()))?;
        let data_start = end
            .checked_add(2)
            .ok_or_else(|| Error::new(ErrorKind::EngineProtocol, "chunk header overflow"))?;
        if size == 0 {
            return Ok(output);
        }
        let data_end = data_start
            .checked_add(size)
            .ok_or_else(|| Error::new(ErrorKind::EngineProtocol, "chunk size overflow"))?;
        output.extend_from_slice(
            input
                .get(data_start..data_end)
                .ok_or_else(|| Error::new(ErrorKind::EngineProtocol, "truncated chunk body"))?,
        );
        let next = data_end
            .checked_add(2)
            .ok_or_else(|| Error::new(ErrorKind::EngineProtocol, "chunk boundary overflow"))?;
        input = input
            .get(next..)
            .ok_or_else(|| Error::new(ErrorKind::EngineProtocol, "truncated chunk terminator"))?;
    }
}
