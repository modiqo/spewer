# Checkpoints turn the design into testable increments

Status: **Accepted**

Implement checkpoints in order. Each checkpoint must pass its automated gate before work starts on the next one.

## Every checkpoint produces the same evidence packet

Each checkpoint records:

- source revision
- commands executed and exit codes
- test and coverage summaries
- fixture or artifact hashes
- design decisions added or changed
- known limitations
- the next checkpoint identifier

Store packets under `artifacts/checkpoints/CP<N>/`. Do not commit secrets or raw model reasoning.

## CP0 freezes the contract and toolchain

**Objective:** Create a reproducible project shell before production behavior exists.

**Deliverables:**

- Pin the Rust toolchain, minimal Tokio features, SQLite library, formatter, linter, dependency policy, and test runner.
- Create the planned source and test directories.
- Add task, event, checkpoint, and receipt schemas from [the task protocol](03-task-protocol.md).
- Record the installed Codex version.
- Generate version-matched App Server JSON schemas and record their aggregate hash.
- Add one recorded happy-path event fixture with secrets removed.

**Acceptance:**

- A clean checkout installs with one documented command.
- Formatting, strict linting, documentation, dependency, source-size, panic-safety, and contract tests pass.
- Every example object in the protocol document validates.
- Generated schema hashes match the recorded manifest.

**Exit gate:** Reviewers accept the product contract, architecture, protocol, and ADRs. Their status changes from **Proposed** to **Accepted**.

## CP1 proves process control and the handshake

**Objective:** Start and stop Codex App Server without creating a task.

**Deliverables:**

- Stdio process supervisor with explicit environment handling.
- JSON-RPC request correlation and notification parsing.
- `initialize` and `initialized` handshake.
- Startup timeout, graceful shutdown, and forced termination.
- `spewer doctor --engine codex` command.

**Acceptance:**

- `spewer doctor --engine codex` reports the Codex version and ready state.
- A malformed line produces a typed protocol error.
- An unknown notification remains observable and does not stop the reader.
- A child process cannot survive a normal supervisor shutdown.

**Exit gate:** The fixture and live process tests pass on macOS and Linux CI.

## CP2 completes one bounded Codex run

**Objective:** Submit one task and receive one normalized terminal result.

**Deliverables:**

- `model/list`, `thread/start`, and `turn/start` requests.
- Configuration for a cheaper available model.
- Event mappings for turns, items, plans, diffs, usage, and reroutes.
- In-memory task projection.
- `spewer run <task.json> --engine codex` command.

**Acceptance:**

- A fixture task creates one file inside an isolated worktree.
- The event stream includes engine binding, turn start, item completion, and turn completion.
- The receipt records requested and observed models.
- The final diff changes only the allowed fixture path.

**Exit gate:** The same contract test passes against a recorded stream and a live Codex installation.

## CP3 makes every accepted event durable

**Objective:** Rebuild current state from SQLite after a process restart.

**Deliverables:**

- Migrations for tasks, attempts, events, source events, and artifacts.
- Transactional event ingestion and deterministic projection reducer.
- Source-event deduplication.
- `spewer status <task-id>` and `spewer tail <task-id>` commands.

**Acceptance:**

- Replaying the event log reproduces the stored projection byte for byte.
- Repeating every source event produces no additional normalized event.
- Killing Spewer after source receipt but before acknowledgement loses no committed event.
- A corrupted projection can be deleted and rebuilt from history.

**Exit gate:** Fault tests pass at every transaction boundary.

## CP4 recovers from interrupted work

**Objective:** Resume or safely reconcile a nonterminal Codex run.

**Deliverables:**

- Checkpoint schema and checkpoint creation policies.
- Codex `thread/read` and `thread/resume` support.
- Workspace revision and diff validation.
- Recovery scanner for nonterminal tasks.
- `spewer resume <task-id>` command.

**Acceptance:**

- Restarting after a completed turn resumes the stored thread.
- Restarting during a turn never reports false completion.
- A mismatched workspace diff blocks automatic resumption.
- A recorded side effect cannot run twice during recovery.

**Exit gate:** The kill matrix in [the test strategy](09-test-strategy.md) passes without manual database edits.

## CP5 makes completion delivery durable

**Objective:** Return a typed receipt even when the parent disconnects.

**Deliverables:**

- Receipt builder and acceptance evidence inventory.
- Outbox and delivery acknowledgement tables.
- Streaming, waiting, and polling callback modes.
- Stable message and receipt identifiers.

**Acceptance:**

- Terminal event, receipt, and outbox row commit together.
- Killing Spewer before delivery preserves the pending message.
- Killing the parent after delivery causes a duplicate callback after restart.
- Applying both callbacks produces one parent-visible result.

**Exit gate:** Delivery tests prove at-least-once transport and idempotent application.

## CP6 enforces budgets and authority

**Objective:** Stop workers that exceed policy without losing their evidence.

**Deliverables:**

- Time, token, tool, retry, and cost budget evaluators.
- Codex interruption and terminal reconciliation.
- Approval and input request path.
- Environment allowlist, path controls, redaction, and effect ledger.

**Acceptance:**

- Each budget has a deterministic boundary test.
- A budget breach interrupts the turn and returns the correct receipt status.
- Approval responses cannot authorize a changed request.
- Path escape, secret leakage, and repeated-effect tests fail closed.

**Exit gate:** Every security test in CP6 passes with network access disabled.

## CP7 emits Pareto IQ evidence

**Objective:** Compare verified quality and cost without hidden assumptions.

**Deliverables:**

- Token, time, tool, model, reroute, and cost records.
- Versioned price configuration.
- Verification result schema and waiver path.
- Machine-readable run export and human summary.

**Acceptance:**

- Missing usage fields remain missing instead of becoming zero.
- A rerouted model appears in the attempt and report.
- Every cost value points to a price configuration hash.
- A report rejects incomparable task classes unless explicitly overridden.

**Exit gate:** A fixture comparison plots two models with traceable quality and cost inputs.

## CP8 proves the parent integration

**Objective:** Let Play or another parent delegate and resume control.

**Deliverables:**

- Typed handoff request and typed receipt adapter.
- Parent event cursor and acknowledgement storage.
- Private continuation field that Spewer treats as opaque.
- End-to-end example with parent disconnection and reconnection.

**Acceptance:**

- The parent retains classification and final-response ownership.
- Spewer receives only projected task context.
- A repeated receipt cannot advance the parent twice.
- The parent can escalate the task to a frontier model with the evidence packet.

**Exit gate:** The integration passes without importing Play into Spewer's core packages.

## CP9 proves that Codex is replaceable

**Objective:** Implement a second fake engine using only the public adapter contract.

**Deliverables:**

- Deterministic fake engine server with plans, tools, pauses, and failure injection.
- Adapter conformance suite.
- Capability negotiation tests.
- Design note for a future Kimi, Qwen, or local-model engine server.

**Acceptance:**

- The fake engine passes task, event, checkpoint, budget, and receipt tests.
- No Codex type appears in core public exports.
- Unsupported capabilities produce explicit fallbacks or request rejection.
- Adding the fake engine changes no public task fixture.

**Exit gate:** The second engine runs the CP2 through CP7 contract suite unchanged.

## CP10 makes the CLI teach its lifecycle

**Objective:** Let an unfamiliar parent choose the next safe command from executable help alone.

**Deliverables:**

- A global task-state diagram and routes for first run, observation, recovery, delivery, and repair.
- Per-command help with usage, timing, state transition, next command, output contract, and example.
- Equivalent `spewer help <command>` and `spewer <command> --help` forms.
- Invalid-input routing back to the global learning surface.

**Acceptance:**

- Every command names its state transition and next safe action.
- Both help forms succeed for every command without opening the database or starting an engine.
- Global help distinguishes JSON from JSON Lines output.
- Invalid commands exit with code 2 and point to `spewer help`.
- Executable help tests, prose lint, and every repository quality gate pass.

**Exit gate:** A parent can derive the normal, observation, recovery, delivery, and repair sequences without external documentation.

## CP11 orchestrates turns over managed App Server workers

**Objective:** Accept work immediately and schedule each active turn against bounded local capacity.

**Deliverables:**

- A foreground local service with a private Unix socket and JSON Lines control protocol.
- Immediate `submit`, observable `load`, and graceful `stop` commands.
- A FIFO scheduler with configurable worker capacity and one active turn per worker.
- Durable `turn.leased` events committed before App Server starts.
- App Server startup, initialization, turn execution, and shutdown owned by the scheduled worker.
- A configured Luna default when a task omits its model.

**Acceptance:**

- Submission returns a durable task handle before App Server starts.
- With worker capacity one, a second task remains queued until the first turn releases capacity.
- A worker failure produces one terminal event, receipt, and outbox message.
- Service shutdown drains accepted work and leaves no App Server child alive.
- The local control socket rejects replacement of a live service and uses owner-only permissions.
- A live task completes through the installed Codex App Server with the configured Luna model.

**Exit gate:** Unit, load, recovery, process, CLI, and live end-to-end evidence pass in one CP11 packet.

## CP12 makes one-off questions immediate

**Objective:** Let a person ask a bounded question without writing a task envelope.

**Deliverables:**

- `spewer init` creates an owner-private configuration under `~/.spewer` without overwriting it.
- `spewer init --overwrite` confirms replacement with an interactive `Y/n` prompt.
- The configuration records the default workspace, engine, model, permissions, and ask budgets.
- `spewer ask <question>` infers a complete task request from the configuration.
- Attached ask returns one structured result and shows live status on terminal standard error.
- Attached ask prints an answer-first view; `--json` selects the structured receipt.
- `--detach` submits through the local service and returns a durable task handle immediately.
- `spewer serve` starts the service in the background and returns JSON after the control socket is ready.
- `spewer serve --foreground` keeps the invoking process attached for debugging and external supervision.

**Acceptance:**

- Initialization creates a `0700` directory and `0600` configuration file on Unix.
- Repeating initialization fails without changing the existing configuration.
- A declined overwrite leaves the existing configuration byte-for-byte unchanged.
- An approved overwrite replaces the file only after the new configuration is durable.
- A missing configuration directs the caller to `spewer init`.
- Ask remains read-only and defaults to `gpt-5.6-luna`.
- Attached progress never contaminates structured standard output.
- Detached ask can be followed through `tail` and `status`; its receipt remains in the outbox.
- Default serve returns its process ID, socket, private log, capacity, and exact next arguments as JSON.
- Repeating serve reports the existing service without starting another process.
- `spewer stop` drains a detached service and removes its control socket.
- A fake App Server test covers initialization through acknowledged callback delivery.
- A live installed `spewer ask` returns a correct answer and usage evidence.

**Exit gate:** CLI, configuration, callback, quality, and live evidence pass in one CP12 packet.

## Release 0.1 requires CP0 through CP12

CP0 through CP12 must pass before the release claims durable orchestration, proven engine portability, or an immediate Luna question path.

## CP13 completes the harness service boundary

**Objective:** Let a harness use every durable task operation through one service-owned interface.

**Deliverables:**

- Capability negotiation for the executable service surface.
- Cursor-based observation combining projection and later events.
- Non-consuming terminal result retrieval by task identifier.
- Idempotent cancellation for queued and running tasks.
- Service-owned receipt acknowledgement.
- JSON CLI projections and lifecycle-directed help for the new operations.

**Acceptance:**

- Observation returns only gap-free committed events after the supplied cursor.
- Result retrieval returns not-ready state before completion and the stable message afterward.
- Acknowledgement does not make a terminal result unqueryable.
- A queued cancellation starts no engine worker.
- A running cancellation stops the worker and leaves one cancelled receipt.
- Repeating cancellation creates no additional terminal event or outbox message.
- Capabilities match the implemented operations and limits.
- CP0 through CP12 gates remain green.

**Exit gate:** Service contract, scheduler cancellation, CLI help, fault, and end-to-end tests pass in one CP13 evidence packet.

CP13 is complete.

## CP14 closes service and harness crash windows

**Objective:** Preserve accepted execution and parent delivery across hard process failure.

**Deliverables:**

- Queue intent committed with task acceptance.
- Durable leases with server epoch, worker identity, expiry, and process custody.
- Startup reconciliation before service readiness.
- Request-hash binding for every idempotency key.
- Consumer-bound receipt acknowledgement.
- Conservative escalation for execution with uncertain evidence.
- Owner-private Play inbox with submit, poll, watch, claim, complete, status, and pending operations.
- Service-directed polling delay for adapters.

**Acceptance:**

- A task accepted before queue insertion runs after restart.
- A lease without workspace or process evidence returns to the queue.
- A task with execution evidence escalates instead of replaying.
- App Server process identity commits before initialization.
- Startup reaps a matching orphan process group and rejects a mismatched identity.
- A changed request cannot reuse an idempotency key.
- Only the declared consumer can acknowledge a message.
- A lost Play submit response returns the original Spewer task after retry.
- Repeated Play claims and acknowledgements do not duplicate application.
- Play command output never exposes its continuation reference.

**Exit gate:** Crash, restart, adapter conformance, quality, and local end-to-end evidence pass in one CP14 packet.


## CP15 makes a useful worker one command away

**Objective:** Install one ready local service and let harnesses discover whether its default capsule is generic or skill-specialized.

**Deliverables:**

- `spewer install` checks or installs the supported Codex CLI, initializes private defaults, creates the default Luna capsule, verifies App Server, and starts the detached service.
- A durable, owner-private capsule manifest with atomic bind and unbind operations.
- Skill identity, revision, and content digest derived from a bound `SKILL.md`.
- Live capsule advertisements in the existing service capability response.
- A deterministic capability content revision for adapter cache invalidation.
- Lifecycle-directed help for installation and capsule management.

**Acceptance:**

- A ready machine reaches a detached Luna service with one command.
- A missing Codex CLI uses the official installer unless the caller opts out.
- Authentication failure gives one exact recovery action and never reports readiness.
- A new installation advertises one generic `default` capsule.
- Binding a skill changes that capsule to `specialized`; unbinding restores `generic`.
- Capability lookup observes a binding change without restarting the service.
- Capsule files and directories use owner-only permissions on Unix.
- Repeating installation does not replace an existing local configuration or start a second service.

**Exit gate:** Unit, CLI, capability lookup, private-file, quality, and local end-to-end evidence pass in one CP15 packet.


## CP16 binds execution to one capsule revision

**Objective:** Make a discovered capsule selection durable, executable, and visible in the terminal receipt.

**Deliverables:**

- An optional capsule selection in the version 0.1 task request.
- A content revision for each advertised capsule.
- Acceptance-time validation of capsule identity, revision, engine, model, and bound skill digest.
- A durable snapshot of specialized skill instructions before the task enters the queue.
- Exact skill instructions in the worker prompt without exposing local source paths.
- Capsule and skill evidence in every receipt for a capsule-bound task.
- Backward compatibility for version 0.1 tasks that omit a capsule.

**Acceptance:**

- A current generic or specialized capsule can be selected and accepted.
- A stale capsule revision fails before task acceptance.
- A changed skill file fails until the owner binds it again.
- Unbinding or editing a skill after acceptance cannot change the queued task snapshot.
- A specialized worker receives the exact bound instructions.
- Its receipt identifies the capsule revision, kind, skill revision, and skill digest.
- Existing version 0.1 task fixtures remain valid without a capsule.

**Exit gate:** Protocol, stale-binding, prompt, receipt, recovery, compatibility, and end-to-end tests pass in one CP16 packet.


## CP17 gives frontier harnesses one small integration surface

**Objective:** Let a frontier harness discover and delegate to Spewer without implementing its control protocol from scratch.

**Deliverables:**

- A public harness client for discovery, capsule-bound delegation, combined checking, and cancellation.
- `spewer delegate` and `spewer check` CLI projections; existing `spewer cancel` completes the three-action model surface.
- Automatic live capability lookup inside delegation.
- Structured outputs that retain task, cursor, capsule, receipt, and acknowledgement identities.
- A concise `spewer-delegation` Agent Skill for Codex and other compatible harnesses.
- Install-time placement of the reference Codex skill without overwriting a changed user file.
- Adapter conformance tests against a running service.

**Acceptance:**

- Delegation selects the requested current capsule and commits one task.
- A stale or missing capsule fails before acceptance.
- Check returns the projection, later events, next cursor, polling delay, and stable terminal message in one response.
- Cancel remains idempotent through the same client.
- The skill tells the frontier model to retain classification, final judgment, and receipt application.
- Repeating installation preserves an identical installed skill and rejects a conflicting file.
- A harness can complete discovery through receipt retrieval without parsing socket frames itself.

**Exit gate:** Client, CLI, skill validation, installation, conformance, help, quality, and end-to-end tests pass in one CP17 packet.


## CP18 adds a production local Qwen3 worker

**Objective:** Run a local Qwen3 capsule through Ollama without changing the public task or receipt schemas.

**Deliverables:**

- An Ollama adapter that implements the accepted engine contract.
- Live Ollama version and model discovery before execution.
- A bounded prompt containing the objective, acceptance criteria, notes, projected files, and exact bound skill snapshot.
- Normalized engine, answer, usage, and terminal events in the existing durable journal.
- One service that routes `codex-app-server` and `ollama` tasks by the selected capsule.
- A capsule command that registers an installed Ollama model without replacing the default Luna capsule.
- An `ask --capsule` path for attached and detached local-model questions.
- Explicit rejection of write or tool-dependent tasks because the first Ollama adapter performs inference without an agent tool loop.

**Acceptance:**

- A live `qwen3:30b-a3b` turn completes through the Ollama adapter.
- The same task and receipt fixtures remain valid without Ollama fields.
- A missing Ollama service or model returns a typed error.
- A generic Qwen3 capsule and a skill-specialized Qwen3 capsule both complete.
- The specialized prompt contains the accepted immutable skill snapshot.
- Attached, detached, cancellation, restart, and receipt retrieval paths retain their existing behavior.
- Capability lookup advertises both engine kinds and observes the Qwen3 capsule without restarting Spewer.
- The receipt identifies Ollama, Qwen3, the observed token counts, and the Ollama version.

**Exit gate:** Adapter, prompt, capsule, service, compatibility, quality, and live local end-to-end evidence pass in one CP18 packet.

## CP19 gives local models bounded web search

**Objective:** Let an Ollama model search without changing public task or receipt schemas.
The [web-search reference](19-bounded-web-search.md) defines scope, limits, events, and tests.
**Exit gate:** Adapter, security, CLI, compatibility, quality, and live evidence pass together.
CP19 is complete. Automated evidence and the user-confirmed live search are under `artifacts/checkpoints/CP19/README.md`.

## CP20 makes plain ask capsule-aware

**Objective:** Make the common question path simple without silently granting network authority.
CP20 is complete. Its evidence is under `artifacts/checkpoints/CP20/README.md`.

## CP21 resolves Ollama's implicit latest tag

**Objective:** Accept Ollama's untagged model shorthand without storing an ambiguous capsule model.

**Acceptance:**

- `mistral` resolves to an installed `mistral:latest`.
- New capsules store the canonical installed model name.
- Explicit tags still require an exact installed match.
- Missing models retain an actionable error.

**Exit gate:** Resolution, capsule creation, live discovery, compatibility, and quality evidence pass. CP21 is complete under `artifacts/checkpoints/CP21/README.md`.
