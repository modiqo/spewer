# Play keeps control while Spewer owns bounded execution

Status: **Accepted**

Play classifies reusable procedures and owns its typed runtime. Spewer does not run `play-machine`, inspect a Play continuation, or decide the parent’s final response.

## The handoff projects one bounded task

The parent creates a `TaskRequest` after it decides that execution is rote enough for a cheaper worker. The request contains the objective, acceptance checks, projected files, permissions, budgets, engine, callback mode, and idempotency key.

Play keeps its continuation identifier in Play’s owner-private runtime. It must not place that identifier in a Spewer task, event, log, or receipt. The generic `private_continuation` field exists for parents whose own contract permits opaque persistence; the Play adapter leaves it empty.

Spewer returns a task handle before it starts Codex. Play may detach after storing the handle and event cursor.

## The callback uses an outbox and parent inbox

Spewer commits a terminal event, typed receipt, and stable outbox message in one SQLite transaction. Delivery may repeat until the parent acknowledges the `message_id` for its `consumer_id`.

Play stores each `receipt_id` it applies. A duplicate callback returns the same receipt but does not advance Play’s event cursor or continuation twice.

The production adapter stores the complete message before it reports readiness. It uses a stable claim ID to bind one harness resume attempt.

See [the Play adapter contract](14-play-adapter.md) for its executable state machine.

The parent can use three modes:

- `stream` prints committed events and the callback on the active process.
- `wait` keeps the process attached until the receipt is durable.
- `poll` lets the parent read pending messages with `spewer outbox <consumer-id>`.

All modes share the same outbox and acknowledgement contract.

## Reversion means returning evidence, not hidden state

Spewer returns the worker summary, observed models, usage, cost provenance, workspace diff, verification evidence, and terminal status. Play or another frontier parent decides whether to accept, verify again, retry, or escalate.

Spewer never returns hidden reasoning. It returns only observable events and artifacts.

## Minimal parent sequence

1. Classify the step and create a bounded `TaskRequest`.
2. Run Spewer and store its task handle.
3. Consume events from the last acknowledged cursor.
4. Apply the typed receipt once by `receipt_id`.
5. Acknowledge the stable outbox `message_id`.
6. Resume Play’s owner-private continuation with the evidence it expects.

This adapter imports no Play package into Spewer’s core crate. The Rust `parent` module provides serializable cursor and receipt-application helpers only.
