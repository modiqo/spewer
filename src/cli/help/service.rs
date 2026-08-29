pub(super) const CAPABILITIES: &str = r"spewer capabilities - negotiate the local service surface

USAGE
  spewer capabilities [--socket <path>]

WHEN
  Use after serve becomes ready and before an adapter depends on optional operations.

STATE
  unknown service surface -> negotiated operations and limits
  This read changes no task or service state.

NEXT
  Confirm the required operations, then use 'spewer submit <task.json>'.

OUTPUT
  One JSON object with protocol version, operations, callback modes, engines, and limits.

EXAMPLE
  spewer capabilities
";

pub(super) const OBSERVE: &str = r"spewer observe - read current state and replay later events

USAGE
  spewer observe <task-id> [--after <seq>] [--socket <path>]

WHEN
  Use after submit or a previous observation while the local service is running.

STATE
  stored cursor -> same task state plus every committed later event -> next cursor

NEXT
  Store next_cursor. Wait poll_after_ms before another observation, or read the terminal result.

OUTPUT
  One JSON object with projection, events, next_cursor, and poll_after_ms. The read is nonblocking.

EXAMPLE
  spewer observe tsk_example --after 42
";

pub(super) const RESULT: &str = r"spewer result - retrieve one task's durable terminal message

USAGE
  spewer result <task-id> [--socket <path>]

WHEN
  Use after observe reports a terminal state or when recovering a detached harness run.

STATE
  nonterminal -> ready false
  terminal message retained -> ready true without acknowledgement

NEXT
  Persist a ready receipt once by receipt_id, then use 'spewer ack <message-id> <consumer-id>'.
  When ready is false, continue other work or observe again later.

OUTPUT
  One JSON object with ready, the current projection, and an optional outbox message.

EXAMPLE
  spewer result tsk_example
";

pub(super) const CANCEL: &str = r#"spewer cancel - durably stop one delegated task

USAGE
  spewer cancel <task-id> [--reason <text>] [--socket <path>]

WHEN
  Use when the parent no longer wants queued or active work to continue.

STATE
  queued or running -> cancelled with one durable receipt
  terminal -> same terminal state without another event

NEXT
  Use 'spewer result <task-id>', persist its receipt, then acknowledge the message.

OUTPUT
  One JSON cancellation result with projection, optional message, and changed.

EXAMPLE
  spewer cancel tsk_example --reason "parent changed direction"
"#;

pub(super) const STATUS: &str = r"spewer status - read the latest durable task projection

USAGE
  spewer status <task-id>

WHEN
  Use when a parent knows the task_id and needs the current state or event cursor.

STATE
  any task state -> same task state
  This read changes no state.

NEXT
  For new events, use 'spewer tail <task-id> --after <event-cursor>'.
  For a terminal task, consume its callback with 'spewer outbox <consumer-id>'.
  After restart, inspect events before using 'spewer resume <task-id>'.

OUTPUT
  One JSON projection with status, phase, event_seq, usage, engine, and workspace evidence. Missing tasks return null.

EXAMPLE
  spewer status tsk_example
";
