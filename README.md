# Spewer

Spewer is a local Rust service that lets your current AI harness delegate bounded work to
lower-cost models.

Keep working in Codex, Claude Code, Kimi, or another preferred harness. Spewer runs the delegated
worker, keeps its task alive, and returns an evidence-rich receipt.

![A frontier harness delegates bounded work through Spewer to a commodity model](assets/tutorial/01-what-spewer-does.png)

The shortest useful path is three commands:

```console
$ cargo install --path . --locked
$ spewer install
$ spewer ask "What is 17 multiplied by 19?"
323
```

That is a working Spewer. The next steps add background work, local Qwen3, frontier delegation,
specialized skills, and concurrent workers.

## Start with one useful worker

You need macOS or Linux, Rust 1.96 or newer, and Git. Spewer installs Codex CLI when it is missing.

Spewer 0.2 uses hosted `gpt-5.6-luna` through Codex App Server. It does not download model weights
to your machine.

### 1. Install Spewer

Install the current checkout:

```console
$ cargo install --path . --locked
```

Prepare Luna, the generic worker capsule, the Codex delegation skill, and the detached service:

```console
$ spewer install
```

A successful response includes `"ready": true` and a generic `default` capsule.

If Codex needs authentication, run `codex` once. Then repeat `spewer install`.

### 2. Ask one foreground question

Run a question and wait for its answer:

```console
$ spewer ask "What is 17 multiplied by 19?"
323
```

This proves that configuration, App Server startup, Luna access, execution, and receipt creation
all work.

Spewer writes progress to standard error. The requested text or structured result stays on
standard output.

### 3. Let a task run in the background

Detach work when you want the caller to continue immediately:

```console
$ spewer ask "Inspect the parser tests and summarize any failures." --detach
```

Spewer returns a durable `task_id`. Check it when convenient:

```console
$ spewer check <task-id>
```

When `ready` becomes `true`, the response contains the stable terminal receipt. Until then, wait
for `observation.poll_after_ms` before checking again.

Follow the worker when you need to debug model or skill activity:

```console
$ spewer watch <task-id>
```

The first lines identify the accepted capsule, engine, and model. They also show its specialization
and skill digest. Codex traces then show safe tool names such as `play-machine`.

If a detached Codex worker needs a date range, approval, or another nonsecret answer,
`spewer check` reports `input_required` and includes `projection.pending_input`. Answer the exact
request without replacing the task:

```console
$ spewer respond <task-id> 99 \
    --response '{"answers":{"dates":{"answers":["August 1–15"]}}}'
$ spewer check <task-id>
```

The bundled frontier skill performs this relay from your existing Codex conversation: it asks you,
records `input.resolved`, and resumes the same worker turn. Spewer rejects credential prompts;
authenticate directly with the provider, then relay only a nonsecret confirmation or choice. An
unanswered input request escalates after 30 minutes and releases the worker. The task wall budget
does not run while a timely human answer is pending.

Ollama traces emit a durable `model active` heartbeat each second until the response arrives. Both
engines show usage and terminal state. `watch` omits hidden reasoning, raw commands, arguments,
tool output, and secrets. Use `spewer tail <task-id>` for the complete machine-readable event log.

Some stateful skills need their existing host caches and owner-private runtime state. For one
explicitly trusted Codex task, disable the sandbox without changing the capsule default:

```console
$ spewer ask "Run the stateful skill" --capsule play-codex \
    --danger-full-access --detach
```

This flag grants that task unrestricted filesystem and network access. `--no-sandbox` is an alias.
It is rejected for Ollama capsules and never applies implicitly to another task.

Cancel work you no longer need:

```console
$ spewer cancel <task-id> --reason "the parent no longer needs it"
```

### 4. Run Qwen3 locally, then optionally add hosted search

Ollama can serve the shipped Qwen3 reference model on your machine. Pull it explicitly because
the model download is large:

```console
$ ollama pull qwen3:30b-a3b
$ spewer doctor --engine ollama --model qwen3:30b-a3b
```

Register the installed model as another capsule:

```console
$ spewer capsule add qwen3-local --engine ollama --model qwen3:30b-a3b
```

List ready capsules with `spewer capsule list`. List every locally installed Ollama model with
`spewer doctor --engine ollama`. Pull another model before registering it:

```console
$ ollama pull mistral
$ spewer capsule add mistral-local --engine ollama --model mistral
```

Ollama stores that model as `mistral:latest`. Spewer resolves the shorter `mistral` name and stores
the canonical installed name in the capsule.

The running service discovers the capsule without restarting. Local inference needs no API key.
Make Qwen3 the capsule used when `--capsule` is absent:

```console
$ spewer capsule default qwen3-local
$ spewer ask "What is 17 multiplied by 19?"
323
```

Without search configuration, its capability card advertises `"network": false` and
`"tools": []`. Frontier adapters keep live-data work when they see those limits.

Missing Ollama telemetry stays missing in receipts. The text view labels cached and reasoning
counts as `not-reported`; an unpriced local run reports `cost=local-unpriced`.

The Ollama worker remains read-only. It receives the objective, notes, projected files, acceptance
criteria, and any bound skill. It rejects commands and file writes.

`OLLAMA_API_KEY` is not required for the local model. It authenticates Ollama's hosted search API.
Set it only when this capsule should support current public information. Restart an older detached
service from the same shell so it inherits that credential:

```console
$ spewer stop
$ spewer serve --engine all
$ spewer capabilities
```

The Qwen capsule now advertises `"network": true` and `"tools": ["web_search"]`. Inspect its
human and machine-readable ask guidance:

```console
$ spewer capsule show
```

Grant network authority explicitly for a current-information question:

```console
$ spewer ask "What is the current weather in Sunnyvale, California?" \
    --web
```

Qwen chooses the query. Spewer validates it, calls Ollama's hosted search API, returns up to five
results, and records the tool call. Local inference stays on the machine; search queries and
results cross the Ollama service boundary.

The Luna capsule named `default` remains available for work that needs the Codex agent tool loop.
Select it for one question with `--capsule default`, or restore it with
`spewer capsule default default`.

`--web` grants request authority only when the capsule advertises `web_search`. Plain attached
questions print answer text and telemetry. Use `--json` for a structured receipt or `--detach` for
a durable task handle. `spewer capsule show <id>` reports these choices for any installed capsule.

### 5. Delegate from Codex without changing harnesses

`spewer install` already installs the reference Codex skill. You do not need a separate
`spewer connect` command.

Ask Codex explicitly for the first proof:

```text
Use Spewer to delegate this bounded task to the default capsule:
inspect the parser tests and return a concise failure summary.
```

The skill uses three Spewer commands:

```console
$ spewer delegate task.json --capsule default
$ spewer check <task-id>
$ spewer cancel <task-id> --reason "the task is no longer needed"
```

Codex keeps the conversation and final judgment. Spewer runs Luna and returns the worker's
receipt.

### 6. Turn a generic worker into a specialist

Bind any valid `SKILL.md` or skill directory to a capsule:

```console
$ spewer capsule bind default /absolute/path/to/review-skill
```

The running service updates immediately. Confirm the new capability card:

```console
$ spewer capabilities
```

The `default` capsule now reports `"kind": "specialized"` with the skill name, revision, and
digest. New tasks receive an immutable instruction snapshot and explicitly activate that skill.

To debug a skill without changing the generic default, create a named Luna capsule and bind it:

```console
$ spewer capsule add play-codex --engine codex-app-server --model gpt-5.6-luna
$ spewer capsule bind play-codex /absolute/path/to/play/SKILL.md
$ spewer ask "play cheat-sheet" --capsule play-codex --detach
$ spewer watch <task-id>
```

The capsule header identifies the accepted Play revision. A `commandExecution/play-machine` line
confirms that Luna invoked the installed Play runtime. Arguments and output remain private.

An interactive Play can keep the same Spewer task while it collects parameters, approval, and
provider authentication. This command starts a concrete Gmail example:

```console
$ spewer ask \
    "Use the exact Play modiqo/retrieve-rideshare-receipts." \
    --capsule play-codex --danger-full-access --detach
$ spewer watch <task-id>
```

The frontier relays nonsecret dates and approval with `spewer respond`. After approval, the Play
can open its scoped OAuth browser from Luna. Complete sign-in in that browser. Credentials, tokens,
cookies, and authorization codes never pass through Spewer responses.

Inferred questions allow 1,000,000 cumulative input tokens by default. Cached context and repeated
tool turns count toward this boundary; it is not a one-million-token context window.

Ask Codex to use it:

```text
Use Spewer's default capsule to review these parser changes.
Apply the bound review skill, then judge the returned receipt.
```

Return the same worker to generic service at any time:

```console
$ spewer capsule unbind default
```

## Run more workers when you need them

One service can lease several local App Server workers concurrently. Restart it with four worker
slots:

```console
$ spewer stop
$ spewer install --max-workers 4
```

`spewer stop` stops new acceptance and drains accepted work first. The next installation starts
the service with the new limit.

Spewer 0.2 scales across local worker processes. Distributed workers on several machines are not
implemented yet.

## Spewer keeps delegated work accountable

The frontier harness owns classification, its private continuation, and the final answer. Spewer
owns accepted work until it can return a terminal receipt.

![A harness adapter stores its continuation while Spewer executes and returns a receipt](assets/tutorial/02-harness-adapter-loop.png)

Four mechanisms make that handoff useful:

- the durable queue keeps accepted tasks after the initiating turn exits;
- permissions and budgets bound worker authority;
- the event journal reconstructs state after a restart;
- receipts identify the capsule, skill, model, usage, artifacts, and verification.

Spewer requeues pristine interrupted work. It escalates work with uncertain external effects
instead of risking duplicate execution.

Cost stays unknown unless `SPEWER_PRICE_CONFIG` points to a matching versioned price file. Spewer
never converts missing price data into zero.

## Know what works today

| Capability | Status |
|---|---|
| Generic Luna worker through Codex App Server | Implemented |
| Foreground questions and detached tasks | Implemented |
| Live generic or specialized capsules | Implemented |
| Immutable skill binding and receipt evidence | Implemented |
| Configurable local worker concurrency | Implemented |
| Reference Codex delegation skill | Implemented |
| Complete durable Play adapter | Implemented |
| Local Qwen3 inference through Ollama | Implemented in CP18 |
| Bounded local-model web search | Implemented in CP19 |
| Persisted default capsule and self-describing ask options | Implemented in CP20 |
| Safe live activity trace for Codex and Ollama | Implemented in CP23 |
| Explicit unsandboxed authority for one Codex task | Implemented in CP24 |
| Same-task typed human input with a 30-minute timeout | Implemented in CP25 |
| Local-model command execution and file writes | Not implemented |
| Native integrations for other frontier harnesses | Planned |
| Distributed multi-machine workers | Not implemented |

Inferred `spewer ask` tasks use read-only filesystem authority and deny network access by default.
`ask --web` is the explicit exception for a capsule that advertises `web_search`.

## Go deeper only when you need to

- [How Spewer works](docs/how_it_works.md) explains the product, every component, and both complete
  user flows.
- [Task protocol](docs/03-task-protocol.md) defines requests, events, receipts, and delivery.
- [Durability](docs/05-durability.md) and [crash closure](docs/13-crash-closure.md) explain restart
  behavior.
- [Security](docs/07-security.md) defines permissions, approvals, and side-effect boundaries.
- [Frontier integration](docs/17-frontier-integration.md) defines the small harness client.
- [Play integration](docs/10-play-integration.md) defines the first complete durable parent
  adapter.
- [Design index](docs/readme.md) links every accepted contract and decision.
- [Checkpoint evidence](artifacts/checkpoints) records passed proof through CP25.

## Build and verify Spewer

Spewer forbids unsafe code, panic primitives, unchecked indexing, and unchecked arithmetic.
Handwritten Rust files stay at or below 500 physical lines.

Run the complete local gate before committing:

```console
$ cargo fmt --all -- --check
$ cargo clippy --all-targets --all-features -- -D warnings
$ cargo test --all-targets
$ RUSTDOCFLAGS="-D warnings" cargo doc --locked --no-deps
$ cargo deny check
$ cargo machete
$ ./scripts/check-rust-source-lines.sh
$ ./scripts/check-doc-lines.sh
$ ./scripts/check-panic-primitives.sh
$ ./scripts/check-codex-schema.sh
```

## License

Apache-2.0. See [LICENSE](LICENSE).
