//! Agent-facing command and lifecycle reference.

mod service;

use super::parse::HelpTopic;
use service::{CANCEL, CAPABILITIES, OBSERVE, RESULT, STATUS};

/// Renders global help or one command reference.
pub(super) fn render(topic: Option<HelpTopic>) -> String {
    let body = match topic {
        None => GLOBAL,
        Some(HelpTopic::Init) => INIT,
        Some(HelpTopic::Ask) => ASK,
        Some(HelpTopic::Doctor) => DOCTOR,
        Some(HelpTopic::Run) => RUN,
        Some(HelpTopic::Serve) => SERVE,
        Some(HelpTopic::Submit) => SUBMIT,
        Some(HelpTopic::Load) => LOAD,
        Some(HelpTopic::Stop) => STOP,
        Some(HelpTopic::Capabilities) => CAPABILITIES,
        Some(HelpTopic::Observe) => OBSERVE,
        Some(HelpTopic::Result) => RESULT,
        Some(HelpTopic::Cancel) => CANCEL,
        Some(HelpTopic::Status) => STATUS,
        Some(HelpTopic::Tail) => TAIL,
        Some(HelpTopic::Rebuild) => REBUILD,
        Some(HelpTopic::Recover) => RECOVER,
        Some(HelpTopic::Resume) => RESUME,
        Some(HelpTopic::Outbox) => OUTBOX,
        Some(HelpTopic::Ack) => ACK,
    };
    body.replace("{version}", env!("CARGO_PKG_VERSION"))
}

const GLOBAL: &str = r#"spewer {version} - delegate bounded work and recover it from durable state

USAGE
  spewer <command> [options]
  spewer help <command>
  spewer <command> --help

TASK STATE
  queued -> starting -> running -> completed | failed | cancelled | escalated
                           \-> input_required | stalled

AGENT ROUTES
  Attached question: init -> ask -> read structured result
  Detached question: serve -> ask --detach -> observe -> result -> ack
  Start service:   doctor -> serve
  Delegate:        capabilities -> submit -> observe -> result -> ack
  One attached:    doctor -> run -> consume receipt -> ack callback
  Observe service: observe --after <event-cursor> -> result
  After restart:   recover -> status or tail -> resume
  Poll delivery:   outbox -> persist receipt once -> ack
  Repair state:    rebuild -> status

COMMON FORMS
  spewer init [--overwrite]             Create or replace private defaults.
  spewer ask "<question>"                Wait and return structured JSON.
  spewer ask "<question>" --text         Wait and print an answer-first view.
  spewer ask "<question>" --detach       Queue work and return a task handle.
  spewer serve --engine codex           Start the service in the background.
  spewer serve --engine codex --foreground  Keep the service attached.

COMMANDS
  init     Write private defaults for inferred question tasks.
  ask      Ask one question through the configured model.
  doctor   Verify Codex App Server before run.
  serve    Run the local turn scheduler and managed App Server workers.
  submit   Commit a task and queue its turn without waiting for completion.
  load     Read scheduler capacity, active turns, and queued turns.
  stop     Stop acceptance and drain the local service.
  capabilities  Read the service operations, limits, and engine kinds.
  observe  Read one projection and replay events after a cursor.
  result   Read one stable terminal message without consuming it.
  cancel   Stop one queued or active task and commit its receipt.
  run      Execute one task from JSON and write JSONL progress.
  status   Read the latest durable task projection. Changes no state.
  tail     Read committed events after a cursor. Changes no state.
  recover  List nonterminal tasks after interruption. Changes no state.
  resume   Reconcile a retained task, then continue from a safe checkpoint.
  outbox   Read callbacks still awaiting one consumer's acknowledgement.
  ack      Mark a durably processed callback as acknowledged.
  rebuild  Recompute a projection from committed events. Use for repair.

OUTPUT CONTRACT
  Commands write data to stdout and diagnostics to stderr.
  run, tail, and outbox write JSON Lines. Other data commands write one JSON value.
  Store the task_id, event cursor, receipt_id, and message_id before advancing a parent.

LEARN THE NEXT STEP
  Run 'spewer help <command>' for WHEN, STATE, NEXT, OUTPUT, and examples.
"#;

const INIT: &str = r#"spewer init - create private defaults for one-off questions

USAGE
  spewer init [--workspace <path>] [--overwrite]

WHEN
  Use once before 'spewer ask'. The current directory is the default workspace.

STATE
  no local configuration -> owner-private ~/.spewer/config.json
  existing configuration -> confirmed replacement | unchanged cancellation

NEXT
  Review the configuration, then use 'spewer ask "<question>"'.
  Use '--overwrite' only when the existing defaults should be replaced.

OUTPUT
  One JSON object with the configuration path and next command.

EXAMPLE
  spewer init --workspace /absolute/path/to/repository
  spewer init --overwrite
"#;

const ASK: &str = r#"spewer ask - infer and run one bounded question task

USAGE
  spewer ask "<question>" [--workspace <path>] [--json | --text]
  spewer ask "<question>" --detach [--socket <path>]

WHEN
  Use attached mode for one result. Use detach while 'spewer serve' is running.

STATE
  attached: question -> inferred task -> progress -> structured result -> acknowledged callback
  detached: question -> queued task handle -> persisted terminal receipt

NEXT
  Attached mode returns the complete result. Use '--text' for an answer-first view.
  Detached mode returns argument arrays for observe, result, and cancel.

OUTPUT
  Attached mode writes one JSON result to stdout and terminal progress to stderr.
  Text mode writes the answer to stdout and telemetry to stderr.
  Detached mode writes one JSON task handle without waiting for App Server.

EXAMPLE
  spewer ask "What is 17 multiplied by 23?"
  spewer ask "Inspect the parser tests" --detach
"#;

const DOCTOR: &str = r"spewer doctor - verify the Codex engine boundary

USAGE
  spewer doctor --engine codex

WHEN
  Use before the first run and after changing the Codex installation.

STATE
  engine unknown -> engine ready
  This command creates no task and changes no durable task state.

NEXT
  If ready is true, use 'spewer run <task.json> --engine codex'.
  If it fails, fix the reported Codex startup or protocol error, then retry doctor.

OUTPUT
  One JSON object with ready, codex_version, and the App Server initialization response.

EXAMPLE
  spewer doctor --engine codex
";

const RUN: &str = r"spewer run - execute one bounded task from JSON

USAGE
  spewer run <task.json> --engine codex

WHEN
  Use after doctor succeeds and the request defines objective, authority, budgets, engine, and callback.

STATE
  new request -> queued -> starting -> running -> terminal receipt
  Terminal means completed, failed, cancelled, or escalated.

NEXT
  Persist the receipt exactly once by receipt_id.
  If output includes a callback, acknowledge it only after the parent commits the receipt.
  After interruption, use 'spewer recover' instead of submitting the same task again.

OUTPUT
  JSON Lines envelopes named handle, event, receipt, and callback.
  The command stays attached until the run finishes or the process is interrupted.

EXAMPLE
  spewer run task.json --engine codex
";

const SERVE: &str = r"spewer serve - run the local turn-aware supervisor

USAGE
  spewer serve --engine codex [--json] [--max-workers <count>] [--socket <path>]
  spewer serve --engine codex --foreground [--max-workers <count>] [--socket <path>]

WHEN
  Default mode returns control after the background service becomes ready.
  Use '--foreground' under a process supervisor or while debugging.
  '--detach' remains an explicit alias for the default. '--json' is also optional.

STATE
  default: service stopped -> background process ready -> JSON result
  foreground: service stopped -> ready -> attached service loop -> draining -> stopped

NEXT
  Use the returned argv arrays for ask, load, or graceful stop.
  Repeating serve reports the existing service without starting another process.

OUTPUT
  Default mode waits only for readiness, writes one JSON object, and exits.
  The object includes started, pid, socket, log, load, and next argv fields.
  Foreground mode writes one JSON readiness line before it waits.

EXAMPLE
  spewer serve --engine codex --max-workers 2
";

const SUBMIT: &str = r"spewer submit - durably queue one task through the local service

USAGE
  spewer submit <task.json> [--socket <path>]

WHEN
  Use while serve is ready. Submission does not wait for App Server or task completion.

STATE
  new request -> queued with durable task handle

NEXT
  Save task_id, then use 'spewer observe <task-id> --after 0'.

OUTPUT
  One JSON task handle after the acceptance event commits.

EXAMPLE
  spewer submit task.json
";

const LOAD: &str = r"spewer load - inspect scheduler capacity

USAGE
  spewer load [--socket <path>]

WHEN
  Use while serve is running to inspect active turns, queued turns, and worker capacity.

STATE
  service state -> same service state

NEXT
  Submit more work only when policy permits, or inspect queued tasks by task_id.

OUTPUT
  One JSON load report. Reading load changes no task or worker state.

EXAMPLE
  spewer load
";

const STOP: &str = r"spewer stop - drain and stop the local service

USAGE
  spewer stop [--socket <path>]

WHEN
  Use when the service should reject new work and finish every accepted turn.

STATE
  accepting -> draining -> stopped

NEXT
  Wait for the control socket to disappear, then run 'spewer serve --engine codex'.

OUTPUT
  One JSON acknowledgement that draining started.

EXAMPLE
  spewer stop
";

const TAIL: &str = r"spewer tail - read committed events after a durable cursor

USAGE
  spewer tail <task-id> [--after <seq>]

WHEN
  Use after status or a previous tail call. Save the highest processed seq as the next cursor.

STATE
  any task state -> same task state
  This read changes no state and may return no lines.

NEXT
  Apply events in sequence order, save the last seq, then call tail again with --after.
  Use 'spewer status <task-id>' when an event indicates a terminal or recovery boundary.

OUTPUT
  Zero or more JSON event lines with gap-free per-task sequence numbers.

EXAMPLE
  spewer tail tsk_example --after 42
";

const RECOVER: &str = r"spewer recover - find tasks that need restart reconciliation

USAGE
  spewer recover

WHEN
  Use after Spewer, its parent, or Codex exits before a terminal receipt becomes durable.

STATE
  retained nonterminal tasks -> same retained nonterminal tasks
  This scan changes no task state.

NEXT
  Inspect each task with status and tail.
  Then use 'spewer resume <task-id>' only when its workspace and checkpoint remain valid.

OUTPUT
  One JSON array of nonterminal task projections. An empty array means no recovery work exists.

EXAMPLE
  spewer recover
";

const RESUME: &str = r"spewer resume - reconcile and continue one interrupted task

USAGE
  spewer resume <task-id>

WHEN
  Use after recover identifies a nonterminal task and status or tail confirms its identity.

STATE
  retained nonterminal -> validate checkpoint and workspace -> running or explicit refusal -> terminal
  Spewer refuses unsafe recovery instead of repeating uncertain work.

NEXT
  Persist the returned receipt exactly once.
  Then read 'spewer outbox <consumer-id>' and acknowledge the committed callback.

OUTPUT
  One terminal receipt as JSON, or a typed error explaining why recovery stopped.

EXAMPLE
  spewer resume tsk_example
";

const OUTBOX: &str = r"spewer outbox - read callbacks pending for one parent consumer

USAGE
  spewer outbox <consumer-id>

WHEN
  Use after a poll-mode run, parent restart, lost connection, or terminal status.

STATE
  callback pending -> callback still pending
  Reading does not acknowledge delivery. Repeated reads may return the same message_id.

NEXT
  Persist and apply each receipt exactly once by receipt_id.
  Then use 'spewer ack <message-id> <consumer-id>'.

OUTPUT
  Zero or more JSON callback lines. Each includes message_id, task_id, receipt_id, and receipt.

EXAMPLE
  spewer outbox play-local
";

const ACK: &str = r"spewer ack - acknowledge one durably processed callback

USAGE
  spewer ack <message-id> <consumer-id> [--socket <path>]

WHEN
  Use only after the named consumer durably stores or applies the callback receipt.

STATE
  callback pending -> callback acknowledged
  Repeating the same acknowledgement leaves it acknowledged and returns applied false.

NEXT
  Continue the parent harness after applied is true or after confirming the receipt was already applied.
  Use 'spewer outbox <consumer-id>' to process another pending callback.

OUTPUT
  One JSON object. applied is true only for the first matching acknowledgement.

EXAMPLE
  spewer ack msg_example play-local
";

const REBUILD: &str = r"spewer rebuild - repair a stored projection from committed history

USAGE
  spewer rebuild <task-id>

WHEN
  Use for projection repair or audit. Do not use it as a normal observation command.

STATE
  event log plus stale projection -> recomputed projection at the same event cursor
  The append-only event history remains unchanged.

NEXT
  Compare the returned projection with expected history.
  Then use 'spewer status <task-id>' or 'spewer tail <task-id> --after <seq>'.

OUTPUT
  One rebuilt JSON projection. A missing task or invalid history returns a typed error.

EXAMPLE
  spewer rebuild tsk_example
";

#[cfg(test)]
mod tests {
    use super::{GLOBAL, render};
    use crate::cli::parse::HelpTopic;

    #[test]
    fn global_help_routes_every_command() {
        for command in [
            "init",
            "ask",
            "doctor",
            "serve",
            "submit",
            "load",
            "stop",
            "run",
            "status",
            "tail",
            "capabilities",
            "observe",
            "result",
            "cancel",
            "recover",
            "resume",
            "outbox",
            "ack",
            "rebuild",
        ] {
            assert!(GLOBAL.contains(command));
        }
    }

    #[test]
    fn global_help_surfaces_common_modes() {
        for flag in ["--overwrite", "--text", "--detach", "--foreground"] {
            assert!(GLOBAL.contains(flag), "missing {flag} from global help");
        }
        assert!(GLOBAL.contains("Wait and return structured JSON"));
        assert!(GLOBAL.contains("Start the service in the background"));
    }

    #[test]
    fn every_command_teaches_its_transition_and_next_step() {
        for topic in HelpTopic::ALL {
            let help = render(Some(topic));
            for section in [
                "USAGE\n",
                "WHEN\n",
                "STATE\n",
                "NEXT\n",
                "OUTPUT\n",
                "EXAMPLE\n",
            ] {
                assert!(help.contains(section), "missing {section} in {topic:?}");
            }
            assert!(help.contains(" -> "), "missing transition in {topic:?}");
        }
    }
}
