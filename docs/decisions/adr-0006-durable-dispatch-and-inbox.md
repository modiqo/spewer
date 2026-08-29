# ADR-0006: pair durable dispatch with a parent inbox

Status: **Accepted**

## Context

The event log and outbox preserve committed task outcomes. They do not preserve an in-memory queue or a harness continuation by themselves.

A process can crash after task acceptance and before queue insertion. A harness can crash after receipt delivery and before acknowledgement.

## Decision

Spewer stores queue intent and worker leases in SQLite. Service startup reconciles this ledger before it reports readiness.

Each harness adapter stores terminal messages in its own durable inbox. The harness claims a receipt, applies it once, and acknowledges Spewer afterward.

Spewer provides consumer-bound, at-least-once delivery. The parent provides idempotent application through a stable receipt and claim identity.

## Consequences

An accepted task cannot disappear with the service process. Work without observable execution evidence can return to the queue.

Work with uncertain evidence escalates instead of repeating. This trades automatic recovery for side-effect safety.

Harness adapters need a small private store. They cannot treat a socket response or model turn as durable state.
