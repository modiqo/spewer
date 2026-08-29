# ADR-0005: keep one service protocol behind thin harness adapters

Status: **Accepted**

## Context

Spewer already has durable task, event, checkpoint, receipt, and outbox contracts. Its local service exposes only submission, load, and shutdown.

CLI observation and delivery commands currently read the database directly. Future Play, MCP, Pi, and other integrations need one complete machine boundary.

## Decision

Spewer will expose one engine-neutral service operation set: capabilities, submit, observe, result, cancel, acknowledge, and load.

CLI, MCP, and native harness adapters will project these operations. They will not implement separate task state machines.

A harness adapter owns host-run correlation, result persistence, turn resumption, and receipt acknowledgement. Spewer owns scheduling and durable task execution.

The local Unix socket remains a replaceable transport. Its encoding does not define task semantics.

## Consequences

Harnesses can integrate without opening Spewer's database. Cursor replay and the outbox survive disconnection.

Each harness still needs a small continuation adapter. That host-specific code remains outside Spewer's core.

MCP and A2A can be added later without changing the durable task contract.
