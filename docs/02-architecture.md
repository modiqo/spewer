# Spewer separates supervision from model-specific control

Status: **Accepted**

Spewer is a local supervisor with one engine-neutral core and one adapter per worker harness. Codex App Server provides the first adapter.

## The control path has one owner at each boundary

```text
Parent harness or Play
        │ TaskRequest
        ▼
Spewer API and task controller
        │ EngineTask
        ▼
Engine adapter ───────► Codex App Server
        │ EngineEvent          │
        ▼                      │ JSON-RPC stream
Event normalizer ◄─────────────┘
        │
        ├── application event log
        ├── task projection
        ├── checkpoint store
        ├── artifact inventory
        └── result outbox ─────► parent callback
```

The parent owns classification and final judgment. Spewer owns delegated work until it produces a terminal receipt.

The harness adapter is outside Spewer's task controller. It stores the parent-to-task association, observes completion, resumes the host, and acknowledges the receipt. See [harness communication](12-harness-communication.md).

## Six components form the version 0.1 system

### The API accepts tasks and exposes handles

The API validates `TaskRequest` before creating durable state. It returns a `TaskHandle` after the initial event commits.

The local interface supports synchronous waiting, event streaming, polling, interruption, and resumption. All modes use the same task identifier.

### The task controller enforces the state machine

The controller applies normalized events to one deterministic state machine. It also enforces time, token, tool, retry, and cost budgets.

The controller never parses Codex-specific payloads. The Codex adapter performs that translation.

### An engine adapter controls one worker harness

Every adapter implements this conceptual interface:

```text
probe(config) -> EngineCapabilities
start(task, workspace) -> EngineHandle
events(handle, cursor) -> stream<EngineEvent>
respond(handle, request_id, response) -> void
interrupt(handle, reason) -> void
inspect(handle) -> EngineState
resume(checkpoint) -> EngineHandle
close(handle) -> void
```

Capabilities describe plans, usage, approvals, resumable threads, live steering, and workspace diffs. The controller branches only on declared capabilities.

### The event normalizer protects the core

The normalizer maps source events to Spewer events. It preserves the original method, identifiers, and payload hash as source metadata.

Unknown source events enter the event log as `engine.unknown`. They never crash the reader or disappear silently.

### Storage provides four distinct records

The application event log records history. The projection answers current-state queries.

The checkpoint store records recovery boundaries. The outbox records messages that still need delivery.

### The workspace manager isolates file effects

Each task receives a dedicated worktree or equivalent isolated directory. The manager records its base revision, current diff hash, and artifact inventory.

Spewer never assumes that a Codex thread restores files. Thread state and workspace state remain separate.

## The task state machine is monotonic

```text
queued -> starting -> running <-> input_required
                        |
                        +-> checkpointed -> running
                        |
                        +-> completed
                        +-> failed
                        +-> cancelled
                        +-> escalated
```

Terminal states never return to `running`. A retry creates a new attempt under the same task identifier.

## Process boundaries remain replaceable

Version 0.1 may run the API, controller, and storage in one process. Their interfaces remain explicit so later versions can separate them.

The Codex adapter may use stdio first and WebSocket later. Transport selection cannot change normalized task behavior.

## Spewer schedules turns against bounded capacity

A turn is the scheduling unit. A thread is the affinity unit. An App Server process is the capacity unit.

The local service accepts a task before it starts App Server. It returns the committed task handle and queues the first turn.

Serve calls `setsid` by default before the service binds its socket. `--foreground` keeps the invoking process attached.

Detached startup redirects standard streams, waits for a successful control request, then returns the process identity and private log path.

The control socket remains the lifecycle authority in both modes. Repeating detached startup reports the ready service instead of spawning a competing scheduler.

Version 0.1 allows one active turn per App Server worker. `max_workers` bounds the number of child processes. Excess turns remain queued in FIFO order.

Before dispatch, Spewer commits a `turn.leased` event with stable lease and worker identifiers. The worker then starts App Server and drives the turn.

A terminal turn releases capacity. A failed worker produces a terminal receipt before the scheduler dispatches the next queued turn.

The worker pool does not change the public task protocol. Callers use stable Spewer task identifiers instead of App Server process, thread, or turn identifiers.

## Ask is a projection, not a second protocol

`spewer ask` converts one question and the local configuration into the existing `TaskRequest` contract.

The projection selects a workspace, read-only authority, budgets, callback ownership, and the configured model. It does not bypass validation, durable acceptance, event storage, budget enforcement, receipt creation, or delivery acknowledgement.

Attached ask writes one typed result to standard output. Terminal progress uses standard error and reads only committed projections.

Text mode may lead with the answer. Detached ask submits the same inferred task to the local supervisor and returns its durable handle.

Callers follow detached work through `tail` or `status`. The terminal receipt remains in the outbox until its consumer acknowledges it.

## Async remains an outer implementation detail

Spewer uses one current-thread Tokio runtime for stdio, timers, signals, bounded channels, and process lifecycle. No durable state depends on a task, future, file descriptor, or live process.

The reducer and budget evaluators are synchronous functions. A dedicated writer thread owns SQLite and completes bounded commands through one-shot replies. No lock or database transaction crosses an `.await` boundary.
