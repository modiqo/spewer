use super::{CliCommand, HelpTopic, parse};
use std::ffi::OsString;
use std::path::PathBuf;

fn args(values: &[&str]) -> Vec<OsString> {
    values.iter().map(OsString::from).collect()
}

#[test]
fn parses_execution_commands() -> Result<(), Box<dyn std::error::Error>> {
    assert_eq!(
        parse(args(&[
            "install",
            "--max-workers",
            "2",
            "--skip-codex-install"
        ]))?,
        CliCommand::Install {
            workspace: None,
            max_workers: 2,
            skip_codex_install: true,
        }
    );
    assert_eq!(
        parse(args(&["capsule", "bind", "default", "/tmp/skill"]))?,
        CliCommand::CapsuleBind {
            capsule_id: "default".to_owned(),
            skill: PathBuf::from("/tmp/skill"),
        }
    );
    assert_eq!(parse(args(&["capsule", "list"]))?, CliCommand::CapsuleList);
    assert_eq!(
        parse(args(&["delegate", "task.json", "--capsule", "default"]))?,
        CliCommand::Delegate {
            path: PathBuf::from("task.json"),
            capsule_id: "default".to_owned(),
            socket: None,
        }
    );
    assert_eq!(
        parse(args(&["check", "tsk_one", "--after", "8"]))?,
        CliCommand::Check {
            task_id: "tsk_one".to_owned(),
            after: 8,
            socket: None,
        }
    );
    assert_eq!(
        parse(args(&["init", "--workspace", "/tmp/example"]))?,
        CliCommand::Init {
            workspace: Some(PathBuf::from("/tmp/example")),
            overwrite: false,
        }
    );
    assert_eq!(
        parse(args(&["init", "--overwrite"]))?,
        CliCommand::Init {
            workspace: None,
            overwrite: true,
        }
    );
    assert_eq!(
        parse(args(&[
            "ask",
            "What is two plus two?",
            "--detach",
            "--socket",
            "/tmp/spewer.sock",
        ]))?,
        CliCommand::Ask {
            question: "What is two plus two?".to_owned(),
            workspace: None,
            capsule_id: None,
            text: false,
            detach: true,
            socket: Some(PathBuf::from("/tmp/spewer.sock")),
        }
    );
    assert_eq!(
        parse(args(&["doctor", "--engine", "codex"]))?,
        CliCommand::DoctorCodex
    );
    assert_eq!(
        parse(args(&["run", "task.json", "--engine", "codex"]))?,
        CliCommand::RunCodex(PathBuf::from("task.json"))
    );
    assert_eq!(
        parse(args(&["serve", "--engine", "codex"]))?,
        CliCommand::Serve {
            max_workers: 1,
            socket: None,
            detach: true,
        }
    );
    assert_eq!(
        parse(args(&["submit", "task.json"]))?,
        CliCommand::Submit {
            path: PathBuf::from("task.json"),
            socket: None,
        }
    );
    Ok(())
}

#[test]
fn parses_ollama_commands() -> Result<(), Box<dyn std::error::Error>> {
    assert_eq!(
        parse(args(&[
            "doctor",
            "--engine",
            "ollama",
            "--model",
            "qwen3:30b-a3b",
        ]))?,
        CliCommand::DoctorOllama {
            model: Some("qwen3:30b-a3b".to_owned())
        }
    );
    assert_eq!(
        parse(args(&["run", "task.json", "--engine", "ollama"]))?,
        CliCommand::RunOllama(PathBuf::from("task.json"))
    );
    assert_eq!(
        parse(args(&[
            "capsule",
            "add",
            "qwen3-local",
            "--engine",
            "ollama",
            "--model",
            "qwen3:30b-a3b",
        ]))?,
        CliCommand::CapsuleAdd {
            capsule_id: "qwen3-local".to_owned(),
            engine: "ollama".to_owned(),
            model: "qwen3:30b-a3b".to_owned(),
        }
    );
    assert_eq!(
        parse(args(&[
            "ask",
            "What is two plus two?",
            "--capsule",
            "qwen3-local",
        ]))?,
        CliCommand::Ask {
            question: "What is two plus two?".to_owned(),
            workspace: None,
            capsule_id: Some("qwen3-local".to_owned()),
            text: false,
            detach: false,
            socket: None,
        }
    );
    Ok(())
}

#[test]
fn parses_durable_queries() -> Result<(), Box<dyn std::error::Error>> {
    assert_eq!(
        parse(args(&["status", "task-one"]))?,
        CliCommand::Status("task-one".to_owned())
    );
    assert_eq!(
        parse(args(&["tail", "task-one", "--after", "12"]))?,
        CliCommand::Tail {
            task_id: "task-one".to_owned(),
            after: 12
        }
    );
    assert_eq!(
        parse(args(&[
            "observe",
            "task-one",
            "--after",
            "12",
            "--socket",
            "/tmp/spewer.sock",
        ]))?,
        CliCommand::Observe {
            task_id: "task-one".to_owned(),
            after: 12,
            socket: Some(PathBuf::from("/tmp/spewer.sock")),
        }
    );
    assert_eq!(
        parse(args(&["result", "task-one"]))?,
        CliCommand::Result {
            task_id: "task-one".to_owned(),
            socket: None,
        }
    );
    assert_eq!(
        parse(args(&["cancel", "task-one", "--reason", "parent stopped",]))?,
        CliCommand::Cancel {
            task_id: "task-one".to_owned(),
            reason: "parent stopped".to_owned(),
            socket: None,
        }
    );
    Ok(())
}

#[test]
fn supports_both_help_forms_for_every_command() -> Result<(), Box<dyn std::error::Error>> {
    for topic in HelpTopic::ALL {
        let name = topic.to_string();
        assert_eq!(
            parse(args(&["help", &name]))?,
            CliCommand::Help(Some(topic))
        );
        assert_eq!(
            parse(args(&[&name, "--help"]))?,
            CliCommand::Help(Some(topic))
        );
    }
    Ok(())
}
