use spewer::protocol::{PROTOCOL_VERSION, TaskInputResponse, TaskRequest};

#[test]
fn task_fixture_decodes() -> Result<(), Box<dyn std::error::Error>> {
    let fixture = include_str!("../fixtures/task-request.json");
    let request: TaskRequest = serde_json::from_str(fixture)?;
    request.validate()?;
    assert_eq!(request.protocol_version, PROTOCOL_VERSION);
    assert_eq!(request.callback.consumer_id.as_deref(), Some("play"));
    Ok(())
}

#[test]
fn callback_consumer_is_required() -> Result<(), Box<dyn std::error::Error>> {
    let fixture = include_str!("../fixtures/task-request.json");
    let mut request: TaskRequest = serde_json::from_str(fixture)?;
    request.callback.consumer_id = None;
    let error = match request.validate() {
        Ok(()) => return Err("missing consumer unexpectedly validated".into()),
        Err(error) => error,
    };
    assert_eq!(error.to_string(), "callback.consumer_id is required");
    Ok(())
}

#[test]
fn explicit_unsandboxed_authority_validates() -> Result<(), Box<dyn std::error::Error>> {
    let fixture = include_str!("../fixtures/task-request.json");
    let mut request: TaskRequest = serde_json::from_str(fixture)?;
    request.permissions.filesystem = "danger-full-access".to_owned();
    request.permissions.network = "allow".to_owned();
    request.validate()?;
    Ok(())
}

#[test]
fn task_input_response_requires_bounded_json() {
    let valid = TaskInputResponse {
        request_id: serde_json::json!(7),
        response: serde_json::json!({"answers":{}}),
    };
    assert!(valid.validate().is_ok());
    let invalid = TaskInputResponse {
        request_id: serde_json::Value::Null,
        response: serde_json::json!("plain text"),
    };
    assert!(invalid.validate().is_err());
}
