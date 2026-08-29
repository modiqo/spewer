# ADR-0008: snapshot a selected capsule before acceptance

Status: **Accepted**

## Context

A capsule manifest and its skill file can change while accepted work waits in the queue. Resolving the current file only when a worker starts would make execution depend on mutable state that was not part of acceptance.

## Decision

The harness selects a capsule by stable ID and content revision. Spewer validates that selection and snapshots its safe evidence and specialized instructions before it commits task acceptance.

The accepted request owns the snapshot for execution and recovery. Receipts copy safe capsule evidence from that snapshot. They omit instruction text and local source paths.

Version 0.1 requests may omit a capsule for backward compatibility. New harness adapters must use capsule-bound requests when they claim routed execution.

## Consequences

Queued work cannot silently change after a bind, unbind, or skill edit. Retries preserve request-hash identity because resolution is deterministic for one capsule revision.

Accepted request storage can contain skill instructions. It remains owner-private and inherits the task store's durability and redaction requirements.
