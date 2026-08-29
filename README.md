# Spewer

**Delegate routine agent work to a fast, inexpensive model. Keep durable state, bounded authority, and verifiable results.**

Spewer is a small Rust supervisor for delegated agent work. A stronger model or another harness describes a bounded task. Spewer runs it through a cheaper model and returns a typed receipt. The receipt contains the answer, state history, token use, elapsed time, artifacts, and verification evidence.

The first production engine is [Codex App Server](https://learn.chatgpt.com/docs/app-server), with `gpt-5.6-luna` as the default model. The task protocol and state machine remain engine-neutral.

## Why Spewer exists

Calvin French-Owen's essay [*Small Models Have Arrived*](https://calv.info/small-models-have-arrived) distinguishes rare “IQ 180” work from abundant “token spewer” work. The latter is responsive, good-enough execution that keeps ordinary tasks moving. The essay also identifies the missing infrastructure around smaller models: harnesses, permissions, and prompt-injection safety.

Spewer is an answer to that infrastructure problem.

A frontier model can explore, classify, and set acceptance criteria. Spewer owns the essential execution loop. It accepts and constrains the task, schedules a turn, persists every transition, survives interruption, and returns evidence. The caller stays responsible for judgment; the delegated worker stays bounded.

```text
caller or agent
      |
      | task / question
      v
 Spewer CLI ---- Unix socket ---- turn scheduler ---- Codex App Server ---- Luna
      |                               |
      +------- SQLite event log ------+
                      |
                receipt + outbox
```

## Pareto profile, not “Pareto IQ”

“Pareto IQ” is evocative, but it is not an established model metric. IQ also suggests a single, general capability score that these runs do not measure.

The precise concept is a **Pareto profile** across quality, cost, and latency. A run belongs on the Pareto frontier when no comparable run improves one of those outcomes without making another worse. Spewer therefore records facts instead of inventing one magic score:

- requested and observed models;

- input, cached-input, output, and reasoning tokens;

- tool calls and wall-clock time;

- priced cost with the exact price-configuration hash;

- passed checks and attempted checks for a declared task class;

- artifacts, verification evidence, and the terminal state.

The telemetry module can turn comparable run exports into plot-ready Pareto points. Comparisons require the same task class, and missing prices remain `unknown` rather than becoming a misleading zero.

“Pareto IQ” can still be Spewer's memorable name for this scorecard. The interface must define it as observed task performance per cost and latency—not intelligence.

## Quick start

Spewer currently targets macOS and Linux. You need Rust 1.96 or newer, Git, and an installed, authenticated Codex CLI that provides `codex app-server`.

```console
$ cargo install --path . --locked
$ cd /path/to/a/git/repository
$ spewer init
$ spewer doctor --engine codex
```

`spewer init` creates owner-private defaults at `~/.spewer/config.json`. The defaults select Luna, deny network access, and make the workspace read-only. They also apply hard limits for wall time, tokens, tools, retries, and cost. To replace an existing configuration interactively:

```console
$ spewer init --overwrite
Overwrite /home/you/.spewer/config.json? [Y/n]
```

Ask a question and wait for its result:

```console
$ spewer ask "What is 1234 multiplied by 41215?"
```

Standard output is one structured JSON result. An interactive terminal shows committed state, token, tool, and elapsed-time progress on standard error. Use `--text` for a human-first answer with telemetry on standard error.

```console
$ spewer ask "What is 1234 multiplied by 41215?" --text
```

## Background work

Start the local scheduler:

```console
$ spewer serve --engine codex --max-workers 1
```

This is nonblocking by default. Spewer waits for the local control socket, then prints one JSON object and exits. The object contains the process ID, private log, current load, and next command arrays. `--foreground` is the explicit blocking mode for debugging or an external process supervisor.

Submit a question without waiting:

```console
$ spewer ask "Inspect the parser tests and summarize the failures" --detach
```

The response contains a durable task ID and exact commands for the next steps:

```console
$ spewer status tsk_example
$ spewer tail tsk_example --after 0
$ spewer outbox spewer-ask
$ spewer ack msg_example spewer-ask
```

The receipt remains in the SQLite outbox until its consumer acknowledges it. A crash or disconnected caller does not turn a completed result into a lost result.

Stop accepting work and drain every accepted turn:

```console
$ spewer stop
```

## Lifecycle contract

```text
queued -> starting -> running -> completed | failed | cancelled | escalated
                           \-> input_required | stalled
```

Spewer appends an event before exposing the corresponding state change. Task state, Codex thread state, and workspace state are separate records. Recovery reconciles those records and refuses uncertain work instead of silently repeating it.

The CLI teaches this protocol in place:

```console
$ spewer help
$ spewer help ask
$ spewer help serve
```

Every command page explains when to use the command, its state transition, the next safe action, and its output contract. Data goes to standard output; diagnostics go to standard error. Harnesses never need to scrape prose to learn a task ID or receipt ID.

## What Spewer abstracts

Spewer does not pretend every model provider has the same wire protocol. It abstracts the orchestration contract around an engine:

| Boundary | Spewer owns |
|---|---|
| Task | objective, acceptance criteria, context, authority, and budgets |
| Engine | start, turn, event mapping, interruption, and identity observation |
| Scheduler | FIFO queue, bounded workers, load, and graceful drain |
| Durability | append-only events, projections, checkpoints, and recovery |
| Delivery | terminal receipts, polling outbox, idempotent acknowledgement |
| Evidence | artifacts, verification, usage, cost provenance, and model identity |

Codex App Server is the first engine because it already supplies a strong open protocol and execution harness. A deterministic fake engine proves the public core is not coupled to Codex. Kimi, Qwen, local models, and other harnesses are future adapters, not implemented backends today.

## Receipts and empty diffs

Every successful receipt contains workspace evidence. A read-only question that changes no files produces the SHA-256 of an empty Git diff:

```json
{
  "kind": "git-diff",
  "sha256": "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
  "uri": "artifact://sha256/e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
}
```

That artifact is evidence that the workspace remained unchanged, not a missing output. Spewer stores hashes and metadata in the public receipt while keeping native prompt and diff bodies private.

## Build and verify

The repository uses deliberately strict Rust constraints: no unsafe code, panic primitives, unchecked indexing, or unchecked arithmetic. Every handwritten Rust source file must remain below 500 physical lines.

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

Checkpoint evidence lives in [`artifacts/checkpoints`](artifacts/checkpoints). The current implementation has completed CP0 through CP12.

## Status

Spewer is early, usable software with a deliberately narrow first path:

- Codex App Server is the only production engine.

- Spewer enforces read-only authority for `ask`, even if someone edits the configuration.

- Detached service mode requires Unix process and socket support.

- Dollar cost requires a versioned price file through `SPEWER_PRICE_CONFIG`.

- The CLI and JSON protocol come first; a future thin adapter can expose them through MCP.

The north star is simple. Capable models spend their attention on judgment. Commodity models handle bounded execution. Every receipt makes the tradeoff visible.

## License

Apache-2.0. See [`LICENSE`](LICENSE).
