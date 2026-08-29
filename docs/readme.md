# Read the Spewer design in sequence

This sequence starts with Spewer's purpose, fixes the public contracts, and ends with tested harness integration.

Run the [README tutorial](../README.md) first. It gives you a working delegated task before these documents explain the machinery.

Read [How Spewer works](how_it_works.md) for the current checkpoint. It joins the implemented system, planned capsule discovery, pluggable engines, and target user experience.

## Stage 1 fixes the product boundary

Read these documents before changing what Spewer owns:

| Step | Document | Question answered | Type |
|---:|---|---|---|
| 01 | [Product contract](01-product-contract.md) | What does Spewer do, and what remains outside it? | Explanation |
| 02 | [Architecture](02-architecture.md) | Which component owns each state and transition? | Explanation |

## Stage 2 fixes the execution contracts

Read these documents before changing a task, engine, store, or permission:

| Step | Document | Question answered | Type |
|---:|---|---|---|
| 03 | [Task protocol](03-task-protocol.md) | What crosses the harness boundary? | Reference |
| 04 | [Codex App Server](04-codex-app-server.md) | How does the first engine run a turn? | Reference |
| 05 | [Durability](05-durability.md) | Which records survive interruption? | Explanation |
| 06 | [Observability](06-observability.md) | Which progress, model, token, and cost facts remain visible? | Reference |
| 07 | [Security](07-security.md) | Which permissions and side effects can a worker use? | Reference |

## Stage 3 proves the implementation

Read these documents before opening or closing a checkpoint:

| Step | Document | Question answered | Type |
|---:|---|---|---|
| 08 | [Implementation checkpoints](08-implementation-checkpoints.md) | Which testable gate comes next? | How-to |
| 09 | [Test strategy](09-test-strategy.md) | Which evidence proves each guarantee? | Explanation |

## Stage 4 connects engines and harnesses

Read these documents before adding a harness, transport, or engine:

| Step | Document | Question answered | Type |
|---:|---|---|---|
| 10 | [Play integration](10-play-integration.md) | How does Play delegate and regain control? | How-to |
| 11 | [Engine adapter](11-engine-adapter.md) | What must every engine implement? | Reference |
| 12 | [Harness communication](12-harness-communication.md) | How do harnesses share one service protocol? | Explanation |
| 13 | [Crash closure](13-crash-closure.md) | How does service and delivery recovery fail safely? | Explanation |
| 14 | [Play adapter](14-play-adapter.md) | How does the first durable adapter store and apply a receipt? | Reference |
| 15 | [Installation and capsules](15-install-and-capsules.md) | How does one command create and advertise a useful worker? | Reference |

## Decisions preserve the reasons

Each accepted decision records one architectural commitment:

1. [Use Codex App Server first](decisions/adr-0001-codex-first.md).
2. [Keep Spewer's protocol engine-neutral](decisions/adr-0002-engine-neutral.md).
3. [Use an application event log and outbox](decisions/adr-0003-event-log-outbox.md).
4. [Use minimal Rust with a bounded Tokio shell](decisions/adr-0004-rust-tokio.md).
5. [Keep one service protocol behind thin harness adapters](decisions/adr-0005-harness-service-boundary.md).
6. [Pair durable dispatch with a parent inbox](decisions/adr-0006-durable-dispatch-and-inbox.md).
7. [Keep capsule manifests durable and capability lookup live](decisions/adr-0007-live-capsule-catalog.md).

## Status words carry fixed meaning

- **Draft:** implementation must not depend on it.
- **Proposed:** reviewers can evaluate a complete direction.
- **Accepted:** implementation can depend on it.
- **Superseded:** another named document replaces it.

The version 0.1 contracts are **Accepted**. A new ADR must supersede any changed public invariant.

## Sources separate facts from choices

[Sources](sources.md) records upstream contracts and retrieval dates. A statement marked **Design choice** describes Spewer rather than upstream behavior.
