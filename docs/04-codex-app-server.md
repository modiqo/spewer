# Codex App Server supplies the first complete engine adapter

Status: **Accepted**

Codex App Server powers rich Codex clients and exposes authentication, history, approvals, and streamed agent events. Its implementation is open source. [Official Codex App Server documentation](https://developers.openai.com/codex/app-server/)

Spewer uses the public protocol as an upstream dependency. Spewer does not expose Codex protocol objects as its public task contract.

## Generated schemas pin each supported Codex version

Codex can generate TypeScript or JSON Schema files that match the installed version. Spewer must generate and commit a fixture manifest for every supported version.

```sh
codex app-server generate-ts --out ./generated/codex
codex app-server generate-json-schema --out ./generated/codex-json
```

The observable result is a versioned schema directory plus a recorded Codex version and content hash.

The adapter must handle unknown notifications without crashing. It records them as `engine.unknown` and increments a compatibility metric.

## The connection handshake precedes every request

The first transport uses stdio because it is local and requires no listening socket. Spewer starts `codex app-server`, sends `initialize`, then sends `initialized`.

Spewer marks the engine ready only after the handshake succeeds. A timeout produces `engine_start_failed` without creating a Codex thread.

## The scheduler owns App Server process lifetime

Spewer starts one App Server child for each leased turn in version 0.1. The configured worker limit bounds concurrent children.

The worker initializes App Server, discovers the model, starts or resumes the thread, and consumes events through terminal turn state. It then shuts down the child before releasing capacity.

Spewer commits the lease before starting the child. App Server process identifiers remain transient and never enter the public task contract.

Future versions may keep workers warm or allow several loaded threads per process. That optimization cannot weaken leases, workspace isolation, or event ordering.

## Model discovery precedes task dispatch

Spewer calls `model/list` before selecting a configured model. It rejects unavailable explicit models and records the returned model metadata.

The task request may name Luna or another cheaper model. Configuration must not assume that every account exposes the same models.

Spewer records `requested_model` at dispatch. A `model/rerouted` notification appends the source and destination models to the receipt.

## Thread identifiers anchor engine recovery

Spewer uses `thread/start` for a new run and stores `thread.id` and `thread.sessionId`. It reads `sessionId` from the response instead of deriving it.

Spewer uses `turn/start` for the bounded objective. The turn configuration supplies the isolated working directory, sandbox policy, and approval policy.

The App Server lifecycle supports `thread/resume` for stored threads and `thread/read` for inspection. Spewer uses those methods during recovery. [Thread lifecycle](https://developers.openai.com/codex/app-server/#start-or-resume-a-thread)

## Source events map into stable Spewer events

| Codex source | Spewer event | Projection effect |
|---|---|---|
| `thread/started` | `engine.bound` | Store thread and `sessionId` |
| `turn/started` | `turn.started` | Set active turn and `running` |
| `turn/plan/updated` | `plan.updated` | Replace the current explicit plan |
| `item/started` | `item.started` | Set the active item |
| output delta notifications | `item.progress` | Update activity and output counters |
| `item/completed` | `item.completed` | Store the authoritative item result |
| `turn/diff/updated` | `workspace.diff.updated` | Store the diff hash and artifact pointer |
| `thread/tokenUsage/updated` | `usage.updated` | Update token counters and budgets |
| approval or input request | `input.required` | Pause the task projection |
| `model/rerouted` | `model.rerouted` | Record observed model history |
| `turn/completed` | `turn.completed` | Set terminal turn status |

Codex states that `item/*` notifications are authoritative for turn items. The adapter must not infer items from the empty arrays in plan or diff updates. [Turn and item notifications](https://developers.openai.com/codex/app-server/#notifications)

## Progress uses evidence Codex actually emits

Plan entries contain `pending`, `inProgress`, or `completed`. Spewer can display a fraction only when the plan contains a stable denominator.

Without a plan, Spewer displays the current item, recent activity, elapsed time, usage, and workspace diff. It does not calculate a completion percentage.

## Requests enter the parent approval path

When Codex requests approval or user input, Spewer stores `input.required` before notifying the parent. The projection includes the native request identifier and typed request shape.

The parent asks the user and calls `respond` with that exact identifier. Spewer validates the
method-specific response, rejects credential prompts, stores `input.resolved`, and sends the
response into the same App Server turn. Human wait time pauses the task wall budget.

An approved skill may start its provider-owned OAuth browser from the delegated Codex turn. The
user completes OAuth there. Spewer never accepts credentials, tokens, cookies, or authorization
codes through `respond`.

An unanswered boundary stalls and escalates after 30 minutes, then releases the worker. A service
crash while waiting still fails closed as uncertain execution; cross-restart continuation of the
live App Server request is not implemented.

## Interruption enforces external budgets

Spewer calls `turn/interrupt` when time, cost, token, tool, or policy limits expire. It waits for the terminal turn event before closing the adapter.

If App Server exits first, Spewer records an engine failure and begins reconciliation. It never converts a missing terminal event into success.

## Codex remains an adapter, not Spewer's identity

Codex-specific methods, statuses, and fields stay under `src/engines/codex`. The core consumes only `EngineCapabilities`, `EngineHandle`, and normalized events.

This boundary lets a future Spewer engine server host Kimi, Qwen, or local models. That server can reuse the thread-turn-item concepts without copying Codex-specific semantics.
