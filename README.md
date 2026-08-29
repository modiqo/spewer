# Spewer

Spewer is a small Rust supervisor that lets agent harnesses delegate bounded work to cheaper models.

The parent harness keeps the judgment. Spewer schedules the routine turn, preserves its state, and returns a typed receipt.

![A parent harness sends a bounded task through Spewer's durable queue, engine adapter, and commodity model. Spewer returns a typed receipt.](assets/tutorial/01-what-spewer-does.png)

Version 0.1 runs [Codex App Server](https://learn.chatgpt.com/docs/app-server) with `gpt-5.6-luna` by default. The public protocol remains independent from Codex.

Neither endpoint is fixed. Another harness can be the parent, and another engine adapter can run a local or remote commodity model.

## Run one bounded task

You need macOS or Linux, Rust 1.96 or newer, Git, and an authenticated Codex CLI.

Install Spewer from this checkout:

```console
$ cargo install --path . --locked
```

Create an owner-private configuration and verify Codex App Server:

```console
$ spewer init
$ spewer doctor --engine codex
```

`spewer init` writes `~/.spewer/config.json`. The defaults choose Luna, deny network access, and make inferred question tasks read-only.

Start one background worker. The command waits for readiness, prints structured JSON, and returns immediately.

```console
$ spewer serve --engine codex --max-workers 1
{
  "ready": true,
  "mode": "detached",
  "max_workers": 1
}
```

Ask a question through the running service:

```console
$ spewer ask "What is 17 multiplied by 19?" --text
323
```

Spewer writes live progress and final telemetry to standard error. Structured task data stays on standard output.

```text
spewer: status=completed model=gpt-5.6-luna ... tools=0 ... cost=unknown
```

Use `spewer init --overwrite` to replace an existing configuration. Spewer asks for confirmation before it writes.

## Read the receipt before trusting the answer

Each terminal receipt identifies the requested and observed models. It also records tokens, tool calls, elapsed time, artifacts, and verification evidence.

That evidence supports a Pareto profile across quality, cost, and latency. “Pareto IQ” is a useful nickname, but it is not a standardized intelligence score.

Spewer never turns missing price data into zero. Set `SPEWER_PRICE_CONFIG` to a versioned price file when dollar cost matters.

An empty Git diff is valid evidence for a read-only task. Its SHA-256 is `e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855`.

## Spewer exists because execution is abundant

Calvin French-Owen's essay [*Small Models Have Arrived*](https://calv.info/small-models-have-arrived) separates rare frontier judgment from abundant “token spewer” work.

Spewer supplies the missing execution layer around that cheaper work:

- a durable queue prevents accepted tasks from disappearing;
- bounded workers keep local load visible;
- permissions and budgets limit worker authority;
- checkpoints preserve observable progress;
- receipts expose model, token, cost, artifact, and verification evidence;
- conservative recovery escalates uncertain effects instead of repeating them.

The decisions explain why each boundary exists:

- [ADR-0001 chooses Codex App Server first](docs/decisions/adr-0001-codex-first.md).
- [ADR-0002 keeps the protocol engine-neutral](docs/decisions/adr-0002-engine-neutral.md).
- [ADR-0003 pairs an event log with an outbox](docs/decisions/adr-0003-event-log-outbox.md).
- [ADR-0004 keeps Rust and Tokio deliberately small](docs/decisions/adr-0004-rust-tokio.md).
- [ADR-0005 puts one service protocol behind thin adapters](docs/decisions/adr-0005-harness-service-boundary.md).
- [ADR-0006 closes dispatch and parent-delivery crash windows](docs/decisions/adr-0006-durable-dispatch-and-inbox.md).

## Run a task without blocking the harness

Detached submission returns a durable handle before a worker starts:

```console
$ spewer ask "Inspect the parser tests and summarize the failures" --detach
```

Store the returned `task_id`. Observe from the last stored event cursor:

```console
$ spewer observe tsk_example --after 0
```

The response includes `next_cursor` and `poll_after_ms`. Wait for that delay before polling again.

Read the stable terminal message after observation reports a terminal state:

```console
$ spewer result tsk_example
```

Persist the receipt before acknowledging its message:

```console
$ spewer ack msg_example spewer-ask
```

Acknowledgement does not delete the result. A later `result` call returns the same terminal message.

Stop new acceptance and drain every accepted turn when local work is finished:

```console
$ spewer stop
```

## A harness uses a thin durable adapter

The harness owns classification, its continuation, and the final response. Spewer owns scheduling, execution, recovery, and receipts.

![A harness classifier and private adapter inbox use submit, observe, result, and acknowledge against Spewer's durable task and receipt outbox.](assets/tutorial/02-harness-adapter-loop.png)

The adapter follows four service operations in order:

1. `submit` commits the task and returns its stable handle.
2. `observe` replays committed events after the adapter's stored cursor.
3. `result` returns one stable terminal message without consuming it.
4. `acknowledge` records that the declared consumer applied the receipt.

Negotiate the exact service surface before depending on an operation:

```console
$ spewer capabilities
```

Submit a complete [task request](docs/03-task-protocol.md), then store its `task_id`:

```console
$ spewer submit task.json
```

Use the same lifecycle through any transport adapter:

```console
$ spewer observe tsk_example --after 0
$ spewer result tsk_example
$ spewer ack msg_example my-harness
```

The adapter must persist its task handle, cursor, terminal inbox entry, claim identity, and application state. It must acknowledge only after its harness durably resumes.

[Harness communication](docs/12-harness-communication.md) defines the portable pattern. [Crash closure](docs/13-crash-closure.md) defines its failure behavior.

## Play is the first conformance adapter

[Play](https://github.com/modiqo/play) implements the durable parent side. It stores private state under `~/.rote-play/spewer`.

The shell tutorial makes every state transition explicit:

```console
$ play spewer submit \
    --host-run-id run_123 \
    --continuation-ref owner_private_ref \
    --request task.json
$ play spewer watch psj_example
$ play spewer claim psj_example --claim-id host_attempt_1
# The harness resumes Play with the claimed receipt.
$ play spewer complete psj_example --claim-id host_attempt_1
```

Production hosts should call the Play adapter in process. This keeps continuation references out of shell history and process arguments.

The [Play integration guide](docs/10-play-integration.md) explains ownership. The [adapter contract](docs/14-play-adapter.md) defines retries, claims, and acknowledgement.

## Recovery never guesses about effects

Spewer commits task acceptance and queue intent in one SQLite transaction. Each worker receives a durable lease before execution starts.

Spewer also records App Server process custody before initialization. Restart verifies that process identity before signaling its process group.

Pristine work returns to the queue. Work with execution evidence becomes `escalated` instead of running twice.

The task lifecycle is monotonic:

```text
queued -> starting -> running -> completed | failed | cancelled | escalated
                           \-> input_required | stalled
```

Read [durability](docs/05-durability.md), [security](docs/07-security.md), and [crash closure](docs/13-crash-closure.md) before adding an engine or transport.

## Read the design in sequence

The [design index](docs/readme.md) starts with the product contract and ends with the tested Play adapter.

The first seven documents explain Spewer and its stable contracts. The remaining documents cover implementation, testing, and integration.

## Build and verify the same gates

Spewer forbids unsafe code, panic primitives, unchecked indexing, and unchecked arithmetic. Every handwritten Rust source file stays below 500 physical lines.

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

Checkpoint evidence lives under [`artifacts/checkpoints`](artifacts/checkpoints). CP0 through CP14 have passed.

## Know the current boundary

Spewer is usable software with a narrow first production path:

- Codex App Server is the only production engine.
- The local service requires Unix process and socket support.
- Inferred `ask` tasks always use read-only filesystem authority.
- Cost stays unknown without a matching versioned price configuration.
- The CLI and JSON service protocol are authoritative.
- MCP can project the same operations later without replacing the protocol.

The next engine can be local or remote. It must satisfy the same task, event, checkpoint, receipt, cancellation, and identity contracts.

## License

Apache-2.0. See [`LICENSE`](LICENSE).
