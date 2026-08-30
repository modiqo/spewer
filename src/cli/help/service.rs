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

pub(super) const RESPOND: &str = r#"spewer respond - continue one task after a human-input boundary

USAGE
  spewer respond <task-id> <request-id> --response '<json>' [--socket <path>]

WHEN
  Use after observe reports input_required and returns projection.pending_input.
  Never place passwords, API keys, access tokens, or other credentials in the response.
  Spewer escalates the task when no response arrives within 30 minutes.

STATE
  input_required -> running on the same task, thread, and worker turn

NEXT
  Continue observing from the saved cursor. Retrieve the result after the task becomes terminal.
  Complete provider OAuth in its browser, then answer only a nonsecret verification prompt.
  After an input timeout, inspect the escalated receipt before starting another task.

OUTPUT
  One JSON projection after Spewer durably records input.resolved.
  A changed request id, secret prompt, incomplete answer, or authority expansion fails closed.

EXAMPLE
  spewer respond tsk_example 7 \
    --response '{"answers":{"dates":{"answers":["August 1–15"]}}}'
"#;

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

pub(super) const TAIL: &str = r"spewer tail - read committed events after a durable cursor

USAGE
  spewer tail <task-id> [--after <seq>]

WHEN
  Use after status or a previous tail call. Save the highest processed seq as the next cursor.

STATE
  any task state -> same task state
  This read changes no state and may return no lines.

NEXT
  Apply events in sequence order, save the last seq, then call tail again with --after.
  Use 'spewer watch <task-id> --after <seq>' for a filtered continuous trace.

OUTPUT
  Zero or more JSON event lines with gap-free per-task sequence numbers.

EXAMPLE
  spewer tail tsk_example --after 42
";

pub(super) const WATCH: &str = r"spewer watch - follow safe model and tool activity

USAGE
  spewer watch <task-id> [--after <seq>]

WHEN
  Use after a detached ask to debug capsule selection and worker activity.

STATE
  stored task -> replay later activity -> terminal state
  This read follows durable events and changes no task state.

NEXT
  Inspect the terminal status, or use tail for the complete machine-readable record.

OUTPUT
  Human-readable lines for capsule, skill, engine, model, safe tools, and heartbeats.
  Hidden reasoning, raw commands, arguments, tool output, and secrets are never printed.

EXAMPLE
  spewer watch tsk_example
";
