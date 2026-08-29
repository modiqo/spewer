use spewer::protocol::{PROTOCOL_VERSION, TaskRequest};

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
