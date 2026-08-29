# The event log makes progress and callbacks recoverable

Status: **Accepted**

Spewer stores every accepted state change before exposing it. The application event log is the durable history for one delegated task.

## SQLite journaling and the application log solve different problems

SQLite WAL mode protects database page updates. Spewer's `events` table records the task history.

Version 0.1 can use one SQLite database in WAL mode. Every state transition still begins with an application event.

## One transaction advances each event

For every accepted source event, one transaction performs these writes:

1. Insert the event using a unique deduplication key.
2. Apply it to the current task projection.
3. Advance the source cursor or source fingerprint set.
4. Add a checkpoint or outbox row when the event crosses a boundary.

Spewer acknowledges the source only after this transaction commits. A duplicate event returns the existing sequence without changing the projection.

## The first schema keeps history and projections separate

| Table | Purpose | Key |
|---|---|---|
| `tasks` | Current task projection | `task_id` |
| `attempts` | One worker attempt and engine handle | `task_id, attempt` |
| `events` | Append-only normalized history | `task_id, seq` |
| `source_events` | Deduplication and raw payload hash | engine source key |
| `checkpoints` | Named recovery boundaries | `checkpoint_id` |
| `artifacts` | Immutable artifact metadata | `artifact_id` |
| `outbox` | Undelivered parent messages | `message_id` |
| `deliveries` | Parent acknowledgements | `message_id, consumer_id` |
| `side_effects` | Idempotency and verification | `effect_key` |

Raw source payload retention is configurable. The default stores hashes and normalized fields while redacting secret-bearing output.

## Checkpoints combine three independent records

A checkpoint references the Codex thread, the Spewer event cursor, and the workspace state. No single record can restore all three.

The checkpoint includes:

- task, attempt, event sequence, and projection version
- engine kind, thread identifier, `sessionId`, and last completed turn
- plan, active item, pending input, and budget counters
- workspace path, base revision, diff hash, and artifact manifest
- completed side-effect keys and pending outbox messages
- recovery policy and checkpoint creation reason

## Only safe boundaries claim resumability

Spewer creates a lightweight checkpoint after important item completions and usage intervals. It marks a checkpoint resumable only at a verified boundary.

Version 0.1 recognizes these resumable boundaries:

- a completed turn
- a stored approval or input request
- an explicit parent pause
- a verified external side effect
- a clean engine shutdown after interruption

An item completion inside an unfinished turn remains evidence. It does not guarantee conversational resumption.

## Recovery reconciles before it acts

After restart, Spewer scans nonterminal tasks. It inspects the engine, event history, workspace, and side-effect records.

The recovery order is fixed:

1. Load the latest resumable checkpoint.
2. Validate the workspace revision and diff hash.
3. Read or resume the stored Codex thread.
4. Reconcile terminal turns and recorded side effects.
5. Rebuild the task projection from the checkpoint cursor.
6. Resume, retry, escalate, or fail under the recorded policy.

Spewer never repeats an external effect before checking its idempotency key. An uncertain effect produces `escalated`, not an automatic retry.

## The outbox makes callback delivery durable

Spewer writes a terminal event, receipt, and `result.ready` outbox row in one transaction. It can crash immediately afterward without losing the callback.

Delivery is at least once. The parent acknowledges the stable `message_id` and `receipt_id` after durable processing.

The outbox retries with bounded exponential delay. A parent can also poll by `task_id` and acknowledge the same message.

## Retention preserves audit value without unbounded growth

Version 0.1 keeps normalized events and receipts until explicit cleanup. Large outputs and diffs live as content-addressed artifacts.

A later retention policy may remove raw payloads after verification. It must preserve event envelopes, hashes, receipts, and acknowledged decisions.
