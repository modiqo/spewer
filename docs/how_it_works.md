# Spewer keeps frontier judgment connected to cheaper repeatable work

Status: **Implemented through CP17**
Date: **2026-08-29**

Spewer is a local delegation service for frontier harnesses. It gives them one durable way to send bounded work to a cheaper worker.

The frontier harness keeps classification, judgment, and the final answer. Spewer owns execution, recovery, and delivery until the harness accepts a receipt.

This document separates the current experience from the machinery that supports it. Each section states what exists now and what remains to build.

## Status words keep the design honest

- **Implemented:** production code and tests already support the behavior.
- **Partial:** the underlying contract exists, but the intended experience or generality does not.
- **Planned:** this checkpoint proposes the behavior; production code does not implement it yet.

The diagrams show logical boundaries. Several boxes can live in one process without losing those boundaries.

## 1. Installation prepares one useful default worker

`spewer install` should leave a user with a working service and a default worker. It should not require them to assemble the architecture.

![Installation prepares the default worker](assets/how-it-works/01-install-worker.png)

### The installer owns setup

The installer checks the host, creates private Spewer directories, and installs the supported worker runtime. It also records the selected engine configuration.

For the first distribution, that runtime is the Codex CLI and its App Server command. The configured default model is `gpt-5.6-luna`.

Spewer installs the runtime that can reach the model. It does not download hosted model weights or place a model inside the Spewer process.

An authenticated Codex installation reaches the hosted model. A future local engine driver may install a local runtime and pull open weights.

### The default capsule is generic

A capsule is a worker description that Spewer can advertise and dispatch. A generic capsule accepts ordinary bounded work without claiming a specialized skill.

A skill binding specializes that capsule. Spewer records the skill name, description, revision, digest, and local source. When a harness delegates through that capsule, Spewer snapshots the exact instructions before accepting the task.

The same worker can therefore advertise `generic` today and `specialized` after a skill binds. The worker does not become a new Spewer implementation.

### Current status and work to build

| Part | Status | Checkpoint |
|---|---|---|
| Codex CLI discovery and `codex app-server --stdio` launch | **Implemented** | Spewer finds `codex` and starts App Server for leased work. |
| Default model `gpt-5.6-luna` | **Implemented** | The task protocol uses this model when the caller does not choose one. |
| One-command `spewer install` | **Implemented** | It finds or installs Codex, initializes defaults, verifies App Server, and starts or reuses the service. |
| Generic capsule record | **Implemented** | Installation persists an owner-private `default` Luna capsule. |
| Skill binding | **Implemented** | Bind and unbind validate `SKILL.md`, then atomically change the advertised state. |
| Local open-weights installation | **Planned** | Let an engine package own runtime installation and model acquisition. |

## 2. The detached service keeps delegated work alive

The `spewer` service survives the terminal or frontier turn that started it. Durable state, rather than a parent process, owns accepted work.

![The detached service owns durable execution](assets/how-it-works/02-detached-service.png)

### Startup is detached by default

`spewer serve` checks the private control socket before it starts another service. If a ready service exists, the command returns that service.

Otherwise, it detaches through `setsid`, redirects output to a private log, and waits for a successful control request. The caller receives the process identity and log path.

### The scheduler leases work to engines

The service accepts a task before it starts a worker process. It commits the task, queues a turn, and returns a stable task handle.

The scheduler leases queued turns against `max_workers`. Version 0.1 starts one App Server process for each active leased turn.

### Storage closes the crash boundary

The event log records history. Projections answer status queries, checkpoints mark recovery boundaries, and the outbox retains terminal receipts.

No accepted task depends on a live shell, file descriptor, or model conversation. A restart reconstructs state from durable records.

### Current status and work to build

| Part | Status | Checkpoint |
|---|---|---|
| Detached `serve`, readiness check, private log, duplicate-start protection | **Implemented** | The service starts detached unless the caller requests foreground mode. |
| FIFO scheduler, worker leases, and bounded capacity | **Implemented** | Spewer starts App Server when a leased turn is ready. |
| Event log, projections, checkpoints, and result outbox | **Implemented** | The durable supervisor and reducer exist. |
| Install-time service start | **Implemented** | `spewer install` starts or reuses the detached service. |
| Host login registration | **Planned** | Add only if real use shows that on-demand detached startup is insufficient. |
| Long-lived warm engine pool | **Planned** | Keep worker runtimes warm only after measurements justify the extra lifecycle. |

## 3. One protocol makes engines and harnesses replaceable

The protocol is Spewer's public product boundary. It should stay small, versioned, and independent of Codex or any frontier harness.

![The protocol separates stable operations from live capabilities](assets/how-it-works/03-protocol.png)

### Stable operations describe the lifecycle

Version 0.1 exposes capabilities, submit, observe, result, cancel, acknowledge, load, and stop. JSON Lines messages cross a private Unix socket.

Task requests carry work, authority, budgets, callback policy, and model selection. Receipts carry the terminal outcome, usage, artifacts, and delivery identity.

### Capability lookup describes the live installation

The protocol schema is generated and versioned with the release. The capability document is looked up from the running service.

That lookup answers what is true now: service version, engine kinds, models, limits, operations, and capsule inventory. An adapter may cache it briefly and refresh after changes.

This split avoids two bad extremes. A frozen adapter misses new bindings, while a fully generated protocol makes compatibility unpredictable.

### Capsules advertise generic or specialized work

The capability document lists each capsule as `generic` or `specialized`. A specialized entry includes its safe skill identity, description, revision, and digest. Local source paths and instruction text remain private.

The frontier harness uses these declarations as evidence for routing. Spewer validates the selected capsule again when it accepts the task.

### Current status and work to build

| Part | Status | Checkpoint |
|---|---|---|
| Versioned service operations over a private Unix socket | **Implemented** | Protocol 0.1 supports the durable task lifecycle. |
| Engine and service capability response | **Implemented** | The service reports its protocol and Codex engine shape. |
| Capsule advertisements in capabilities | **Implemented** | Lookup returns generic or specialized workers and safe skill metadata. |
| Capsule selection in `TaskRequest` | **Implemented** | A request selects a capsule ID and revision; Spewer validates and snapshots it before acceptance. |
| Dynamic capsule lookup and cache invalidation | **Implemented** | Every lookup reads current manifests and returns a deterministic content revision. |
| Compatibility policy | **Implemented** | Requests without a capsule retain version 0.1 behavior; routed requests bind an exact live revision. |

## 4. A harness adapter preserves the frontier harness's control

A harness adapter is host-side integration code. It translates a harness's lifecycle into Spewer operations and preserves the host's private continuation.

![The harness adapter bridges a frontier harness to Spewer](assets/how-it-works/04-harness-adapter.png)

### The adapter is not a model or router

The adapter submits tasks, follows events, stores terminal receipts, resumes the host, and acknowledges delivery. It does not decide which work deserves delegation.

It also keeps private host state out of Spewer. A continuation token belongs to the harness that created it.

### A stable shell can use dynamic lookup

Most adapter code is written once against the versioned service protocol. At startup or first use, it asks Spewer for current capabilities and capsules.

New bindings therefore appear without regenerating the adapter. Regeneration is needed only when the stable protocol version or host integration contract changes.

### The adapter closes two durable handoffs

The first handoff stores intent before submission. The second stores the receipt before the host resumes.

Acknowledgement happens only after the frontier harness durably accepts the result. Retries return the same task, receipt, and claim.

### Current status and work to build

| Part | Status | Checkpoint |
|---|---|---|
| Durable Play adapter with submit, watch, claim, complete, and retry safety | **Implemented** | Play stores bindings and receipts in an owner-private SQLite inbox. |
| Small model-visible surface | **Implemented** | `delegate`, `check`, and `cancel` hide ordinary lifecycle plumbing. |
| Reusable harness client | **Implemented** | The Rust client provides discovery, capsule-bound delegation, combined checking, and cancellation. |
| Durable host inbox | **Implemented for Play** | Other hosts must persist their private continuation and receipt application through their own adapter. |
| Adapter conformance kit | **Partial** | Play and the reference client pass service tests; cross-host certification remains. |
| Capsule-aware routing lookup | **Implemented** | Delegation reads the live catalog, selects an advertised revision, and fails safely when it is missing or stale. |

## 5. A frontier plugin gives the model a small Spewer surface

The plugin is the harness-specific package a user installs. It contains the adapter connection and brief instructions for when delegation is appropriate.

![A frontier plugin exposes three tools and live capsule lookup](assets/how-it-works/05-frontier-plugin.png)

### Tools perform actions

The reference Codex skill uses `spewer delegate`, `spewer check`, and `spewer cancel`. A native host plugin can project the same three actions as tools.

A native host adapter should hide socket paths, event cursors, inbox rows, claims, and acknowledgements. The reference CLI retains structured lifecycle fields for deterministic integrations, but its skill only requires the task ID.

### A minimal skill teaches selection

A small Spewer integration skill explains the delegation boundary. It tells the frontier model to delegate bounded, checkable work and retain final judgment.

Specialized worker skills do not need to be copied into the frontier context. Capability lookup exposes their safe identity, description, and revision. Spewer injects the exact bound instructions only into the selected worker task.

The frontier model can then discover a suitable generic or specialized capsule through the adapter. The adapter returns match evidence, not an opaque routing decision.

### Connection should follow installation

Users should not need a general `spewer connect <frontier-harness>` command. Each harness already has its own plugin installation mechanism.

For Codex, `spewer install` places the reference skill into the configured Codex skill directory. This is installation, not a new architectural connection layer.

### Current status and work to build

| Part | Status | Checkpoint |
|---|---|---|
| Play-side Spewer adapter commands | **Implemented** | The host can call the durable lifecycle today. |
| Reference Spewer integration skill | **Implemented** | The bundled skill teaches bounded delegation and preserves frontier judgment. |
| Codex skill installation | **Implemented** | Setup installs identical content idempotently and refuses to overwrite a changed file. |
| Native Codex, Claude, Cursor, and other plugin packages | **Planned** | Package the same client contract when a host needs richer native tools or durable continuation. |
| Required `connect` command | **Not needed** | Live discovery happens during delegation. |

## 6. Pluggable ends let Spewer moderate repeatable work

Spewer should connect any compliant frontier harness to any compliant worker engine. The durable middle remains the same when either end changes.

![Pluggable frontier and worker ends meet at Spewer](assets/how-it-works/06-pluggable-ends.png)

### The frontier side supplies judgment

Frontier models are best reserved for work that needs broad context, novel reasoning, ambiguous tradeoffs, or final accountability. Their harnesses decide what to delegate.

### The worker side supplies throughput

Cheaper models can handle repeatable work when the task has a clear contract, bounded tools, useful examples, and a checkable result. A specialized skill can improve that fit.

Spewer calls this moderation rather than automatic replacement. The frontier keeps control and can reject, retry, refine, or complete the work itself.

### Evidence improves future routing

Every receipt can record capsule identity, skill revision, model, latency, usage, verifier outcome, and frontier acceptance. Those facts support later routing policy.

Learning must change declared policy, not silently rewrite authority. A route can become cheaper only after repeated evidence shows that its result remains acceptable.

### Engine drivers isolate worker differences

Each engine driver probes capabilities, starts work, normalizes events, responds to approvals, interrupts, inspects, resumes, and closes. Spewer's controller sees only normalized events.

A Codex driver can reach hosted Luna today. A future driver can reach an open-weights runtime without changing harness adapters or receipts.

### Current status and work to build

| Part | Status | Checkpoint |
|---|---|---|
| Engine-neutral core and conceptual engine interface | **Implemented** | The controller does not parse Codex-specific payloads. |
| Codex App Server driver | **Implemented** | This is the only production worker engine. |
| Fake engine seam | **Implemented** | Tests prove scheduler and recovery behavior without Codex. |
| Additional hosted or open-weights drivers | **Planned** | Define packaging, installation, authentication, and capability probes. |
| Policy-based capsule matcher | **Planned** | Rank eligible capsules while exposing reasons and preserving frontier choice. |
| Outcome learning | **Planned** | Record verifier and acceptance evidence before adjusting routing defaults. |

## 7. One delegated request crosses every boundary

The complete flow remains understandable when every component appears together. This example delegates a repeatable release-note draft to a specialized worker.

![A complete request crosses the frontier plugin, adapter, Spewer, and worker](assets/how-it-works/07-connected-use-case.png)

### The frontier discovers a suitable capsule

The user asks the frontier harness to prepare a release. The frontier plugin asks its adapter for current Spewer capabilities.

The lookup returns a `release-notes` capsule bound to a known skill revision. The frontier selects it because the task is bounded and the result is reviewable.

### Spewer executes under an explicit contract

The adapter stores its private continuation and submits the request. Spewer validates the capsule digest, authority, workspace, and budgets before committing the task.

The scheduler leases the turn to the Codex driver. App Server runs Luna with the selected skill and emits progress, usage, approvals, and artifacts.

Spewer normalizes those events and persists them. A worker crash can fail or resume the task without losing the accepted handle.

### The frontier receives evidence, then judges

Spewer writes one terminal receipt to its outbox. The adapter stores that receipt in its inbox before it resumes the frontier harness.

The frontier reviews the draft and evidence. It may accept, revise, retry with another capsule, or handle the task itself.

After the host accepts the result, the adapter acknowledges the receipt. Spewer can then close delivery without owning the user's final answer.

### Current status and work to build

| Flow segment | Status | Checkpoint |
|---|---|---|
| Submit through durable execution to a terminal receipt | **Implemented** | Spewer and the Play adapter prove the lifecycle. |
| Frontier continuation, claim, and acknowledgement | **Implemented for Play** | Other harness packages do not exist yet. |
| Skill-aware discovery and capsule selection | **Implemented** | The client selects a live revision; Spewer snapshots and executes its exact binding. |
| Luna started through Codex App Server | **Implemented** | Spewer starts one App Server per leased turn. |
| Verifier outcome and acceptance feedback | **Planned** | Extend receipts without weakening deterministic replay. |

## 8. The user experience hides the architecture

The product succeeds when users can benefit from these boundaries without learning them first. Setup should have one default path and reveal details only when needed.

![The user sees install, ask, and result while Spewer handles the machinery](assets/how-it-works/08-user-experience.png)

### First use should take one command

The target experience starts with this command:

```sh
spewer install
```

The observable result is a ready local service, a working generic Luna capsule, a reference Codex delegation skill, and clear authentication status.

The user then asks their frontier harness normally. The plugin discovers Spewer and delegates only when the request fits its policy.

### Specialization should be optional and reversible

An advanced user can bind a skill to a generic capsule through an explicit command or plugin action. Unbinding restores the generic advertisement.

The capability revision changes immediately. Connected adapters discover the new state on their next lookup without reconnecting or regenerating code.

### Failures should name the next action

Missing authentication should point to sign-in. An unavailable capsule should show eligible alternatives, and a stopped service should offer one safe start command.

Users should never diagnose event cursors, worker leases, or outbox rows to complete ordinary work. Those details exist for operators and recovery tools.

### Adoption constraints

- A default installation must be useful before the user binds a skill.
- A frontier plugin must discover the service without a required `connect` ceremony.
- The model-visible interface must stay at three ordinary actions.
- Every delegation must remain inspectable, cancellable, and attributable.
- Advanced engines, skills, and policies must remain optional layers.

### Current status and work to build

| Experience | Status | Checkpoint |
|---|---|---|
| Manual init, doctor, serve, and ask | **Implemented** | The individual lifecycle commands remain available for operators. |
| Detached service behavior | **Implemented** | Repeated startup returns the existing ready service. |
| One-command installation and default capsule | **Implemented** | One command prepares Codex, configuration, capsule, and detached service. |
| Skill-led discovery and delegation in Codex | **Implemented** | The installed skill uses the three CLI actions and live capsule lookup. |
| Native integration in other frontier hosts | **Planned** | Each host can package the same client while retaining its private continuation. |
| Capsule management CLI | **Implemented** | List, bind, and unbind cover the first explicit administration path. |
| Capsule management UI | **Planned** | Add richer UI only after use proves the need. |

## The checkpoint keeps one simple promise

Install Spewer once, then ask a frontier model for work as usual. Spewer should make cheaper repeatable execution available without moving judgment, safety, or recovery into prompts.

CP16 binds accepted work to an immutable capsule snapshot. CP17 gives frontier harnesses the three-action client and installs the reference Codex skill. The next build slice is CP18: prove the engine boundary with a second production worker.

## Sources

- [Codex CLI](https://learn.chatgpt.com/docs/codex/cli) describes the supported standalone CLI installation and sign-in flow.
- [Codex App Server](https://learn.chatgpt.com/docs/app-server) describes `codex app-server`, its default stdio transport, and its initialization handshake.
- [Codex skills](https://learn.chatgpt.com/docs/build-skills) describes skill metadata and progressive disclosure.
- [Spewer architecture](02-architecture.md), [engine adapter](11-engine-adapter.md), [harness communication](12-harness-communication.md), and [Play adapter](14-play-adapter.md) define the accepted local contracts summarized here.
