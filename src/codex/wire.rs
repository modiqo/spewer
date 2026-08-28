use crate::error::{Error, ErrorKind, Result};
use serde_json::{Value, json};
use tokio::io::{AsyncWrite, AsyncWriteExt};

pub(super) enum ParsedLine {
    Response {
        id: String,
        result: Result<Value>,
    },
    Notification {
        method: String,
        params: Value,
    },
    ServerRequest {
        id: Value,
        method: String,
        params: Value,
    },
    Malformed(String),
}

pub(super) fn parse_line(line: &str) -> ParsedLine {
    let message: Value = match serde_json::from_str(line) {
        Ok(value) => value,
        Err(error) => return ParsedLine::Malformed(error.to_string()),
    };
    let method = message.get("method").and_then(Value::as_str);
    let id = message.get("id");
    if let Some(method) = method {
        let params = match message.get("params") {
            Some(params) => params.clone(),
            None => json!({}),
        };
        return match id {
            Some(id) => ParsedLine::ServerRequest {
                id: id.clone(),
                method: method.to_owned(),
                params,
            },
            None => ParsedLine::Notification {
                method: method.to_owned(),
                params,
            },
        };
    }
    let Some(id) = id else {
        return ParsedLine::Malformed("JSON-RPC message has neither method nor id".to_owned());
    };
    let id_key = match serde_json::to_string(id) {
        Ok(key) => key,
        Err(error) => return ParsedLine::Malformed(error.to_string()),
    };
    if let Some(error) = message.get("error") {
        return ParsedLine::Response {
            id: id_key,
            result: Err(Error::new(ErrorKind::EngineProtocol, error.to_string())),
        };
    }
    match message.get("result") {
        Some(result) => ParsedLine::Response {
            id: id_key,
            result: Ok(result.clone()),
        },
        None => ParsedLine::Malformed("JSON-RPC response has no result or error".to_owned()),
    }
}

pub(super) async fn write_message(
    writer: &mut (impl AsyncWrite + Unpin),
    value: &Value,
) -> Result<()> {
    let bytes = serde_json::to_vec(value)?;
    writer.write_all(&bytes).await?;
    writer.write_all(b"\n").await?;
    writer.flush().await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{ParsedLine, parse_line};

    #[test]
    fn malformed_line_stays_observable() {
        assert!(matches!(parse_line("not-json"), ParsedLine::Malformed(_)));
    }

    #[test]
    fn unknown_notification_stays_a_notification() {
        let line = r#"{"method":"future/event","params":{"value":1}}"#;
        assert!(matches!(
            parse_line(line),
            ParsedLine::Notification { method, .. } if method == "future/event"
        ));
    }
}
