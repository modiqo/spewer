//! Help for the model-visible frontier harness commands.

pub(super) const DELEGATE: &str = r"spewer delegate - discover, bind, and submit one task

USAGE
  spewer delegate <task.json> [--capsule <id>] [--socket <path>]

WHEN
  Use for bounded, checkable work after 'spewer install'. The default capsule ID is 'default'.

STATE
  task request -> live capsule lookup -> revision-bound queued task

NEXT
  Store the task ID, then use 'spewer check <task-id> --after <cursor>'.

OUTPUT
  One JSON object with the durable handle, catalog revision, and selected capsule advertisement.

EXAMPLE
  spewer delegate task.json --capsule default
";

pub(super) const CHECK: &str = r"spewer check - observe progress and retrieve a stable result

USAGE
  spewer check <task-id> [--after <event-cursor>] [--socket <path>]

WHEN
  Use after delegate while the frontier harness continues other work.

STATE
  stored cursor -> later committed events and current terminal result

NEXT
  If ready is false, wait poll_after_ms and check again.
  A durable adapter can store next_cursor and pass it through --after.
  Persist a terminal receipt before 'spewer ack <message-id> <consumer-id>'.

OUTPUT
  One JSON object containing ready, observation, and non-consuming result snapshots.

EXAMPLE
  spewer check tsk_example
";
