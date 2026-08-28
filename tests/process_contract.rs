//! App Server process ownership and protocol contract tests.

use serde_json::json;
use spewer::codex::{CodexClient, CodexConfig, CodexMessage};
use std::ffi::OsString;
use std::path::PathBuf;
use std::time::Duration;

fn fake_config(script: &str) -> CodexConfig {
    CodexConfig {
        program: PathBuf::from("sh"),
        app_server_args: vec![OsString::from("-c"), OsString::from(script)],
        inherited_environment: Vec::new(),
        startup_timeout: Duration::from_secs(2),
        request_timeout: Duration::from_secs(2),
        shutdown_timeout: Duration::from_secs(2),
    }
}

#[tokio::test(flavor = "current_thread")]
async fn unknown_notification_is_observable_and_shutdown_waits()
-> Result<(), Box<dyn std::error::Error>> {
    let script = r#"
        IFS= read -r initialize
        printf '%s\n' '{"id":1,"result":{"ready":true}}'
        IFS= read -r initialized
        printf '%s\n' '{"method":"future/event","params":{"ok":true}}'
        IFS= read -r request
        printf '%s\n' '{"id":2,"result":{"data":[]}}'
        while IFS= read -r line; do :; done
    "#;
    let mut client = CodexClient::connect(fake_config(script)).await?;
    let result = client.request("model/list", json!({})).await?;
    assert_eq!(result, json!({"data": []}));
    let message = client.next_message().await;
    assert!(matches!(
        message,
        Some(CodexMessage::Notification { method, .. }) if method == "future/event"
    ));
    client.close().await?;
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn malformed_line_does_not_stop_reader() -> Result<(), Box<dyn std::error::Error>> {
    let script = r#"
        IFS= read -r initialize
        printf '%s\n' '{"id":1,"result":{"ready":true}}'
        IFS= read -r initialized
        printf '%s\n' 'not-json'
        while IFS= read -r line; do :; done
    "#;
    let mut client = CodexClient::connect(fake_config(script)).await?;
    let message = client.next_message().await;
    assert!(matches!(message, Some(CodexMessage::Malformed { .. })));
    client.close().await?;
    Ok(())
}
