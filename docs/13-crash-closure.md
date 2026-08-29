# Crash closure makes accepted work recoverable

Status: **Accepted**

Spewer treats a crash as a state transition, not an erased process. Every accepted task must restart safely or produce an explicit escalation receipt.

## Three ledgers close different crash windows

SQLite WAL protects committed database pages. Spewer's dispatch ledger protects accepted work before a worker starts.

The outbox protects a terminal receipt before the parent receives it. The parent inbox protects the harness continuation before acknowledgement.

```text
task + dispatch intent
        ↓
lease + process custody
        ↓
terminal event + receipt + outbox
        ↓
parent inbox + continuation application
        ↓
acknowledgement
```

Each arrow permits retries. No arrow assumes cross-process exactly-once delivery.

## Acceptance and queue intent share one transaction

The `tasks`, first `events`, `attempts`, and `dispatches` rows commit together. A crash cannot leave an accepted task outside the durable queue.

An in-memory queue is only a scheduling cache. Service startup rebuilds that cache before it reports readiness.

## Idempotency keys bind the complete request

Spewer stores a canonical request hash with each idempotency key. The hash excludes a generated `task_id` and includes fields that change worker behavior.

An identical retry returns the first task handle. A changed objective, budget, permission, callback, engine, or context returns `invalid_input`.

## A lease records custody before execution

The scheduler commits `turn.leased` with the lease row in one transaction. The row records the server epoch, worker identity, deadline, and process custody.

Codex App Server starts without receiving protocol data. Spewer records its process group before sending `initialize`.

This order closes the gap between process creation and observable engine work.

## Restart uses evidence, not optimism

Startup classifies every dispatch before the service becomes ready.

| Durable evidence | Startup action |
|---|---|
| Queued task with no workspace or process | Queue it again |
| Lease with no workspace or process | Return it to the queue |
| Workspace, process, or engine evidence exists | Reconcile as uncertain |
| Terminal task still names a process | Reap the process and retain the result |

Spewer verifies the recorded executable signature before signaling a process group. A mismatch fails startup instead of killing an unrelated process.

An uncertain nonterminal task becomes `escalated`. Spewer writes the event, receipt, and outbox message together and does not repeat the work.

## Delivery remains at least once

Every task must declare a nonempty `callback.consumer_id`. Spewer filters pending delivery by that identity and binds acknowledgement to it.

Another harness cannot list or acknowledge the message by guessing its identifier.

The harness stores the receipt before acknowledging it. A duplicate receipt must return the stored application result without advancing the harness twice.

## External effects require their own keys

Spewer cannot infer whether an arbitrary external command completed before a crash. An engine adapter must record planned, started, verified, or uncertain effects.

Spewer never retries an uncertain effect automatically. The terminal receipt carries the escalation evidence for a frontier model or person.

## Crash tests define completion

The kill matrix covers these boundaries:

1. Before and after acceptance commit.
2. After lease commit and before process creation.
3. After process registration and during App Server work.
4. Before and after terminal outbox commit.
5. After parent inbox commit and before acknowledgement.
6. After acknowledgement commit and before its response.

A passing restart produces a safe continuation, the same terminal result, or an explicit escalation. Silent loss and automatic uncertain-effect replay fail the checkpoint.
