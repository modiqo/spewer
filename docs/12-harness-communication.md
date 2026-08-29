# Harnesses communicate through one durable Spewer service

Status: **Accepted**

Spewer owns task execution and durable state. A parent harness owns conversations, turns, and final judgment.

The integration boundary has two layers. The Spewer service exposes task operations. A small harness adapter translates terminal receipts into the host's continuation mechanism.

## The service protocol and harness adapter solve different problems

The service protocol answers this question:

> What happened to this durable task?

The harness adapter answers another:

> What should this harness do now that the task changed?

Spewer must not import Play, Pi, Claude, Kimi, or another harness runtime. An adapter must not recreate Spewer's scheduler or task state machine.

```text
frontier model
      │
model-visible delegation tool
      │
harness adapter
  ├── stores the task handle
  ├── observes completion outside model context
  ├── resumes the correct harness turn
  └── acknowledges the stored receipt
      │
Spewer service protocol
      │
scheduler, journal, checkpoints, receipts, and outbox
      │
Codex App Server or another engine
```

## One operation set supports every transport

Version 0.1 exposes these service operations:

| Operation | Input | Output | Durable effect |
|---|---|---|---|
| `capabilities` | None | Protocol version, operations, limits, engines, and callback modes | None |
| `submit` | Validated `TaskRequest` | Durable `TaskHandle` | Accepts and queues a task |
| `observe` | Task ID and event cursor | Projection, later events, and next cursor | None |
| `result` | Task ID | Current status and optional terminal outbox message | None |
| `cancel` | Task ID and reason | Terminal projection | Commits cancellation and a receipt once |
| `acknowledge` | Message ID and consumer ID | Whether this consumer applied a new acknowledgement | Records delivery |
| `load` | None | Queue depth and worker capacity | None |

`stop` remains a local service lifecycle operation. It is not part of delegated task semantics.

CLI, MCP, and native harness adapters must call the same Rust application methods. They cannot write task state independently.

## Observation combines projection and replay

`observe` returns the current projection and all committed events after the supplied cursor.

```json
{
  "projection": {
    "task_id": "tsk_example",
    "status": "running",
    "event_seq": 8
  },
  "events": [],
  "next_cursor": 8
}
```

The operation is nonblocking in version 0.1. A caller chooses its polling interval. A later additive version may accept a bounded wait duration.

Streams and notifications may reduce latency. They never replace cursor replay.

## Result retrieval never consumes delivery

`result` returns the latest task status and its terminal outbox message when ready.

Reading a result does not acknowledge it. The harness must first store or apply the receipt. It then calls `acknowledge` with its stable consumer identity.

This separation preserves delivery across a crash between receipt retrieval and parent persistence.

## Cancellation is durable and idempotent

A queued cancellation removes the task from the scheduler before a worker starts. A running cancellation stops the worker and its App Server child.

Spewer then commits one `task.cancelled` event, one cancelled receipt, and one outbox message. Repeating cancellation returns the existing terminal projection without creating another event or receipt.

A completion that commits before cancellation wins remains completed. A cancellation that commits first prevents later worker events from changing the terminal state.

## The adapter owns host continuation

A harness adapter stores this private association:

```text
harness run identity <-> Spewer task identity and event cursor
```

The association must not enter the delegated worker prompt. Play keeps its owner-private continuation outside Spewer.

The adapter follows this sequence:

1. Submit work and durably store the returned handle.
2. Observe from the last stored event cursor.
3. Retrieve the terminal result.
4. Deduplicate and store the receipt by `receipt_id`.
5. Resume the harness through its native continuation mechanism.
6. Acknowledge the outbox `message_id`.

The adapter can retry every step. Spewer tasks and receipt application remain idempotent.

The adapter must persist an inbox row before resuming the host. It must bind that row to one stable claim identity.

See [crash closure](13-crash-closure.md) for service recovery and [the Play adapter](14-play-adapter.md) for the first conformance implementation.

## Model-visible tools stay smaller than the service protocol

A language model should express intent instead of operating queue mechanics. A harness may expose only these tools:

```text
spewer_delegate
spewer_check
spewer_cancel
```

The host runtime owns acknowledgement and routine polling. These operations should not consume frontier context.

An MCP server is a projection of this service. MCP Tasks may improve compatible clients, but Spewer's journal and outbox remain authoritative.

## Capability negotiation prevents accidental assumptions

The capability response declares:

- Spewer protocol version

- supported service operations

- callback modes

- engine kinds

- maximum control-message bytes

- cancellation support

- cursor replay support

Adapters must inspect capabilities before using optional operations. An unsupported operation returns an explicit error instead of silently degrading.

## The local socket is transport, not task semantics

Version 0.1 uses newline-delimited JSON over an owner-private Unix socket. One connection carries one request and one response.

The message envelope may evolve into JSON-RPC without changing task, observation, result, cancellation, or acknowledgement semantics. Windows may use a named pipe or another owner-private local transport.

## CP13 proves the boundary

The implementation checkpoint must prove:

- every operation uses the service-owned database and state machine

- observation returns gap-free events after a caller cursor

- result retrieval remains available after acknowledgement

- queued cancellation starts no engine process

- running cancellation leaves no App Server child

- repeated cancellation creates no duplicate terminal event or receipt

- repeated acknowledgement is harmless

- capabilities describe the executable service surface

- the existing CLI and CP0 through CP12 tests remain compatible

MCP and a production Play runtime adapter remain later projections. CP13 prepares their stable substrate.
