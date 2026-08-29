# CP12 evidence: attached and detached questions

Status: **Passed**

Starting revision: `aa61f681ab5af9ce10d8b6d4020205008831f42b`

Date: 2026-08-28

## Implemented behavior

- `spewer init` writes versioned defaults to `~/.spewer/config.json`.

- Initialization infers the current directory unless `--workspace` supplies an override.

- The configuration records workspace, App Server engine, Luna model, read-only permissions, and hard budgets.

- Initialization uses create-new semantics by default.

- `--overwrite` requires an interactive `Y/n` confirmation and replaces only the approved file version.

- `spewer ask` infers the existing `TaskRequest`; it does not introduce another execution protocol.

- Attached ask prints one JSON object containing the answer, task identity, status, and complete receipt.

- An attached terminal displays committed state, token, tool, and elapsed-time progress on standard error.

- `--text` selects the answer-first view.

- `--detach` submits through the local service and immediately returns a JSON task handle with exact follow-up commands.

- `spewer serve` calls `setsid` and returns JSON after the control socket responds.

- `--foreground` retains the attached service loop for debugging and external supervision.

- Detached startup reports process identity, private log, current load, and next-command argv arrays.

- Detached receipts remain in the SQLite outbox until their consumer acknowledges them.

- Attached ask flushes its result before acknowledging the durable callback.

## Automated results

| Command | Exit | Result |
|---|---:|---|
| `cargo fmt --all -- --check` | 0 | Formatting passed |
| `cargo clippy --all-targets --all-features -- -D warnings` | 0 | Strict linting passed |
| `cargo test --all-targets` | 0 | 40 tests passed |
| `RUSTDOCFLAGS="-D warnings" cargo doc --locked --no-deps` | 0 | Public documentation passed |
| `cargo deny check` | 0 | Advisory, license, ban, and source policy passed |
| `cargo machete` | 0 | No unused dependency found |
| Repository source, panic, document, and schema scripts | 0 | All local gates passed |

Tests cover safe replacement, both output formats, detached startup, idempotent startup, private logs, detached submission, polling receipts, and callback acknowledgement.

## Live installed result

The installed command retained `~/.spewer` with mode `0700` and `config.json` with mode `0600`. The configuration selected the Spewer repository, read-only filesystem access, denied network access, and `gpt-5.6-luna`.

`spewer init --overwrite` displayed `Overwrite $HOME/.spewer/config.json? [Y/n]`. Answering `n` returned a structured cancellation and left the configuration SHA-256 unchanged. Pressing Enter approved an atomic replacement. The resulting SHA-256 remained `489550dbae9ae0116ea71c578c0438ad5399ab5e1d2cd4044b70a534b433c48f` because the generated defaults were identical.

This command ran through the installed Codex App Server:

```text
spewer ask "What is 7 multiplied by 8? Answer with only the number."
```

The terminal displayed live `queued`, `starting`, and `running/acting` states on standard error. Standard output was a parseable JSON object with answer `56`, completed status, task ID, and the complete receipt.

Telemetry reported `gpt-5.6-luna`, 17,251 input tokens, 9,984 cached input tokens, 40 output tokens, 33 reasoning tokens, zero tool calls, and 4,569 milliseconds. The task identifier was `tsk_48ecbef50f6ca2be3d7e7ad7`.

This installed command started service process `51076` without holding the invoking shell:

```text
spewer serve --engine codex --max-workers 1 --detach --json
```

The JSON result reported `ready: true`, `started: true`, the socket, mode-`0600` log, empty load, and argv arrays. A later process showed parent ID `1` and `setsid` isolation, proving it survived the starting CLI.

Repeating the command returned `started: false` and the existing load. It did not start a competing scheduler.

The background service then accepted this detached question:

```text
spewer ask "What is 13 multiplied by 7? Answer with only the number." --detach
```

Task `tsk_781953499026ff79ea37e2a3` completed with answer `91`. The polling receipt was `rcp_7edcf47b64a55fd614c1ef1c`.

The run observed `gpt-5.6-luna`, 17,239 input tokens, 9,984 cached tokens, 29 output tokens, 22 reasoning tokens, zero tools, and 5,777 milliseconds.

Acknowledging message `msg_d3744f9fed8be9c0f97f5db4` removed the receipt. `spewer stop` then removed the socket and process. Provider cost remained unknown because no matching price configuration was installed.

The final installed interface also ran the exact default command:

```text
spewer serve --engine codex --max-workers 1
```

It returned structured JSON in 0.4 seconds and left process `60032` ready with parent ID `1`. A separate `load` succeeded. Repeating the command returned `started: false`, and `stop` removed the socket.

The final global help exposes `--overwrite`, attached JSON and text output, detached questions, background service startup, and explicit `--foreground` mode. Its contract tests keep those common forms synchronized with command-specific help.

## Artifact hashes

| Artifact | SHA-256 |
|---|---|
| `src/config.rs` | `2cc195c1dd27918f382a80167e297331414e6ac7802cbea8235c0c86f33c3747` |
| `src/cli.rs` | `41705b430fb42c54fb07dbd58f4a3aaab285335d9e330dc4fc6350bcb600ab7e` |
| `src/cli/service.rs` | `53670bcf617700c68b9583a1aaf0f0485682e5a1a592859911a9bca5807c6f7f` |
| `src/cli/question.rs` | `fc793a3a067f11ee74a98446551ee7dc6c43cb2bdafaea0a63efda1bf3645562` |
| `src/cli/parse.rs` | `61452bcdc92b722baace24918edcda04033c64551d9aaaa46634459581af0012` |
| `src/cli/parse/service.rs` | `369a0f60e48c11f330d7f91cdf05a6ba2cabc16bc70a8102f9ab43d0dac61b53` |
| `src/cli/parse/question.rs` | `ac8fa6bea92b3b6c2915b9e72f16c8370d76335a5c9533a83c37e9ff65865fa1` |
| `src/cli/help.rs` | `a67e431f149d511295be289d2caa255aad85616e00c3cf64e4a63e19c694fd3f` |
| `tests/cli_help.rs` | `34f224210a84fe8cd39c6e1c9a34fb95778575839143e997b67eebf50ec06acb` |
| `tests/local_service.rs` | `4d69938cf53c4d9d9bcb90d8ef04f07935bd9e3842e807956ba03c28e690ca62` |
| `README.md` | `0d18332918f08d93edc182c24a97b1744b6439711179d1f2bd8d2509b5f8a591` |
| Installed release binary | `f7ff4da0dab5e92693fbcf4e1342c57e695be5420a82ea00a7a585917c1c760f` |

## Design decisions

- Ask is a convenience projection over the public task protocol.

- Ask authority is always read-only, even if someone edits the configuration.

- JSON is the default contract so harnesses can consume every result without output scraping.

- Terminal progress uses standard error and committed projections, so it cannot corrupt standard-output JSON or invent percentages.

- Text mode remains an explicit convenience for people.

- Detached ask switches the callback to polling and lets the existing service own execution.

- Serve detaches and returns JSON by default. `--detach` and `--json` remain explicit aliases.

- `--foreground` is the only blocking service mode.

- A detached service calls `setsid` before it binds the socket.

- The control socket proves readiness and remains the lifecycle authority.

- Repeated detached startup is idempotent and exposes the existing load.

- The outbox, not the service process, owns terminal-response persistence.

- A missing price stays unknown rather than becoming a fabricated zero.

## Known limitations

- Ask requires an existing Git workspace because version 0.1 retains worktree isolation.

- Configuration has one global layer. Project and command overlays are future work.

- Attached ask starts a cold App Server process and remains attached until its terminal receipt.

- Detached ask requires a running service. The default `spewer serve` starts it without holding the terminal.

- `tail` is cursor-based and one-shot. A caller repeats it with the last event sequence rather than holding a streaming connection.

- Dollar cost requires a matching versioned price configuration.

## Documentation review

Desk route 4 applied because README, help, design, and evidence are project documentation. The linter reported no failures.

- `WAIVE S-01`: Markdown evidence lists retain exact counters; the linter joins adjacent items.
- `WAIVE S-04`: Protocol, command, stream, model, and lifecycle names are fixed technical terms.
- `WAIVE S-05`: Passive wording describes repository state where another actor would reduce clarity.
- `WAIVE T-02`: Reference lists are independently scannable rather than prose paragraphs.

The skim extract carries setup, question execution, output channels, evidence, and limitations in order. Verdict: pass.

The target prompts cover nonblocking startup, JSON startup results, asynchronous questions, task tails, and persisted receipts. The README and command help answer them directly.

No applicable Modiqo competitor claim appears in these project documents.

Next checkpoint: **review configuration layering, price installation, and the thin Play adapter**
