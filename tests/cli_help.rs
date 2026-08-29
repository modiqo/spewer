//! Executable contract tests for agent-facing help and error routing.

use std::process::Command;

#[test]
fn executable_help_teaches_the_global_and_command_routes() -> Result<(), Box<dyn std::error::Error>>
{
    let binary = env!("CARGO_BIN_EXE_spewer");
    let global = Command::new(binary).arg("help").output()?;
    assert!(global.status.success());
    let global = String::from_utf8(global.stdout)?;
    assert!(global.contains("TASK STATE\n"));
    assert!(global.contains("AGENT ROUTES\n"));
    assert!(global.contains("COMMON FORMS\n"));
    assert!(global.contains("doctor -> serve"));
    assert!(global.contains("capabilities -> submit -> observe -> result -> ack"));
    for flag in ["--overwrite", "--text", "--detach", "--foreground"] {
        assert!(global.contains(flag), "missing {flag} from global help");
    }

    let command = Command::new(binary).args(["run", "--help"]).output()?;
    assert!(command.status.success());
    let command = String::from_utf8(command.stdout)?;
    assert!(command.contains("new request -> queued -> starting -> running -> terminal receipt"));
    assert!(command.contains("After interruption, use 'spewer recover'"));
    for command in [
        "init",
        "ask",
        "serve",
        "submit",
        "load",
        "stop",
        "capabilities",
        "observe",
        "result",
        "cancel",
    ] {
        let output = Command::new(binary).args([command, "--help"]).output()?;
        assert!(output.status.success());
        let help = String::from_utf8(output.stdout)?;
        assert!(help.contains("STATE\n"));
        assert!(help.contains("NEXT\n"));
    }
    let serve = Command::new(binary).args(["serve", "--help"]).output()?;
    let serve = String::from_utf8(serve.stdout)?;
    assert!(serve.contains("--foreground"));
    assert!(serve.contains("background process ready -> JSON result"));
    Ok(())
}

#[test]
fn ask_without_configuration_teaches_initialization() -> Result<(), Box<dyn std::error::Error>> {
    let home = std::env::temp_dir().join(format!("spewer-missing-config-{}", std::process::id()));
    let output = Command::new(env!("CARGO_BIN_EXE_spewer"))
        .args(["ask", "What is two plus two?"])
        .env("HOME", home)
        .output()?;
    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8(output.stderr)?;
    assert!(stderr.contains("run 'spewer init'"));
    Ok(())
}

#[test]
fn invalid_command_points_to_the_learning_surface() -> Result<(), Box<dyn std::error::Error>> {
    let output = Command::new(env!("CARGO_BIN_EXE_spewer"))
        .arg("unknown")
        .output()?;
    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8(output.stderr)?;
    assert!(stderr.contains("Run 'spewer help'"));
    let conflict = Command::new(env!("CARGO_BIN_EXE_spewer"))
        .args(["serve", "--engine", "codex", "--detach", "--foreground"])
        .output()?;
    assert_eq!(conflict.status.code(), Some(2));
    Ok(())
}
