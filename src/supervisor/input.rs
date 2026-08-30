//! Validation for one durable human-input boundary.

use crate::error::{Error, ErrorKind, Result};
use crate::protocol::TaskInputResponse;
use serde_json::Value;

pub(super) fn validate(pending: &Value, reply: &TaskInputResponse) -> Result<()> {
    reply.validate()?;
    if pending.get("request_id") != Some(&reply.request_id) {
        return Err(invalid(
            "input response does not match the pending request id",
        ));
    }
    let request = pending
        .get("request")
        .ok_or_else(|| invalid("pending input has no request payload"))?;
    if contains_secret_prompt(request) {
        return Err(invalid(
            "Spewer does not accept credentials; complete authentication out of band and respond only after verification",
        ));
    }
    match pending.get("method").and_then(Value::as_str) {
        Some("item/tool/requestUserInput") => validate_questions(request, &reply.response),
        Some("mcpServer/elicitation/request") => validate_elicitation(&reply.response),
        Some("item/commandExecution/requestApproval" | "item/fileChange/requestApproval") => {
            validate_approval(&reply.response)
        }
        Some("item/permissions/requestApproval") => Err(invalid(
            "runtime permission expansion is unsupported; submit a new task with explicit authority",
        )),
        Some(method) => Err(invalid(format!(
            "Spewer cannot answer unsupported App Server request {method}"
        ))),
        None => Err(invalid("pending input has no request method")),
    }
}

fn validate_questions(request: &Value, response: &Value) -> Result<()> {
    let questions = request
        .get("questions")
        .and_then(Value::as_array)
        .ok_or_else(|| invalid("user input request has no questions"))?;
    let answers = response
        .get("answers")
        .and_then(Value::as_object)
        .ok_or_else(|| invalid("user input response requires an answers object"))?;
    if answers.len() != questions.len() {
        return Err(invalid(
            "user input response must answer every question once",
        ));
    }
    for question in questions {
        let id = question
            .get("id")
            .and_then(Value::as_str)
            .ok_or_else(|| invalid("user input question has no id"))?;
        let values = answers
            .get(id)
            .and_then(|answer| answer.get("answers"))
            .and_then(Value::as_array)
            .ok_or_else(|| invalid(format!("user input response has no answer for {id}")))?;
        if values.is_empty() || !values.iter().all(Value::is_string) {
            return Err(invalid(format!(
                "answers for {id} must be nonempty strings"
            )));
        }
    }
    Ok(())
}

fn validate_elicitation(response: &Value) -> Result<()> {
    match response.get("action").and_then(Value::as_str) {
        Some("accept") if response.get("content").is_some() => Ok(()),
        Some("decline" | "cancel") => Ok(()),
        Some("accept") => Err(invalid("accepted elicitation requires content")),
        _ => Err(invalid(
            "elicitation action must be accept, decline, or cancel",
        )),
    }
}

fn validate_approval(response: &Value) -> Result<()> {
    match response.get("decision").and_then(Value::as_str) {
        Some("accept" | "decline" | "cancel") => Ok(()),
        _ => Err(invalid(
            "approval decision must be accept, decline, or cancel for this request",
        )),
    }
}

fn contains_secret_prompt(value: &Value) -> bool {
    match value {
        Value::Object(fields) => fields.iter().any(|(key, value)| {
            (key == "isSecret" && value.as_bool() == Some(true))
                || (key == "format" && value.as_str() == Some("password"))
                || contains_secret_prompt(value)
        }),
        Value::Array(values) => values.iter().any(contains_secret_prompt),
        Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_) => false,
    }
}

fn invalid(message: impl Into<String>) -> Error {
    Error::new(ErrorKind::InvalidInput, message)
}

#[cfg(test)]
mod tests {
    use super::validate;
    use crate::protocol::TaskInputResponse;
    use serde_json::json;

    #[test]
    fn answers_exact_nonsecret_questions() {
        let pending = json!({
            "request_id": 7,
            "method": "item/tool/requestUserInput",
            "request": {"questions":[{"id":"dates","question":"Date range?","isSecret":false}]}
        });
        let reply = TaskInputResponse {
            request_id: json!(7),
            response: json!({"answers":{"dates":{"answers":["August 1–15"]}}}),
        };
        assert!(validate(&pending, &reply).is_ok());
    }

    #[test]
    fn rejects_credentials_and_changed_request_identity() {
        let secret = json!({
            "request_id": "input-one",
            "method": "item/tool/requestUserInput",
            "request": {"questions":[{"id":"key","question":"API key?","isSecret":true}]}
        });
        let reply = TaskInputResponse {
            request_id: json!("input-one"),
            response: json!({"answers":{"key":{"answers":["do-not-store"]}}}),
        };
        assert!(validate(&secret, &reply).is_err());
        let mut changed = reply;
        changed.request_id = json!("input-two");
        assert!(validate(&secret, &changed).is_err());
    }
}
