# ADR-0003: Spewer owns an application event log and outbox

Status: **Accepted**

Date: 2026-08-28

## Context

Worker events, file effects, process crashes, and parent callbacks cross separate failure boundaries. Database journaling alone cannot explain or replay task progress.

Spewer must recover without assuming that either Codex or the parent remained connected.

## Decision

Spewer appends every accepted task transition to an application event log. It derives the current task projection from those events.

Spewer writes terminal receipts and callback messages through a transactional outbox. Parents process callbacks idempotently and acknowledge stable message identifiers.

## Consequences

Spewer supports restart recovery, progress replay, durable callbacks, and auditable cost records. It must manage event retention and projection migrations.

Delivery is at least once. Exactly-once effects come from stable keys and idempotent consumers, not transport promises.

## Rejected alternatives

Keeping only mutable task rows loses the sequence that explains recovery decisions. Treating SQLite WAL files as task history couples product state to storage internals.

Sending callbacks before committing a receipt can lose results. Committing receipts without an outbox can strand results after a crash.

## Review trigger

Review this decision if measured event volume exceeds local SQLite limits or a remote deployment requires partitioned storage.
