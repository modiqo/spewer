# CP11 evidence: bounded App Server orchestration

Status: **Passed**

Starting revision: `aa61f681ab5af9ce10d8b6d4020205008831f42b`

Date: 2026-08-28

## Implemented behavior

- `serve` owns the database, a private Unix socket, the FIFO scheduler, and every Codex App Server child.
- `submit` commits task acceptance before it returns a stable handle or queues work.
- `load` reports active turns, queued turns, configured capacity, accepted tasks, completed workers, failed workers, and drain state.
- `stop` rejects new work, drains accepted turns, closes storage, and removes the socket.
- Each worker commits `turn.leased` before starting one App Server process for one active turn.
- Tasks that omit `engine.model` request `gpt-5.6-luna`.
- Worker startup and protocol failures produce a terminal event, receipt, and outbox message.

## Automated results

| Command | Exit | Result |
|---|---:|---|
| `cargo fmt --all -- --check` | 0 | Formatting passed |
| `cargo clippy --all-targets --all-features -- -D warnings` | 0 | Strict linting passed |
| `cargo test --all-targets` | 0 | 35 tests passed |
| `RUSTDOCFLAGS="-D warnings" cargo doc --locked --no-deps` | 0 | Public documentation passed |
| `cargo deny check` | 0 | Advisory, license, ban, and source policy passed |
| `cargo machete` | 0 | No unused dependency found |
| Repository source, panic, document, and schema scripts | 0 | All local gates passed |

The repository does not configure a coverage percentage gate. Tests cover queue capacity, duplicate submission, worker failure finalization, socket ownership, live-socket rejection, CLI lifecycle, recovery, callbacks, and process shutdown.

## Live Codex App Server results

The live probe used `codex-cli 0.150.1`, the installed App Server, capacity one, and a task that omitted its model.

The first run used a 20,000-token ceiling. Spewer observed `gpt-5.6-luna` and returned an `escalated` receipt at the token boundary. It recorded 35,430 input tokens, 465 output tokens, 181 reasoning tokens, and three tool calls. The isolated worktree had zero changed files.

The completion run used a 100,000-token ceiling. It returned `completed` in 8,797 milliseconds and identified the package correctly. It recorded 35,138 input tokens, 274 output tokens, 118 reasoning tokens, and one tool call. The receipt named `gpt-5.6-luna` as requested and observed. Its empty diff hash was `e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855`.

Both callbacks survived service shutdown. The parent read and acknowledged each message, after which its outbox was empty. Graceful stop removed the control socket.

The verified source was installed to the existing Cargo binary location. The installed `spewer` passed `doctor`, reported App Server ready, and completed `serve -> load -> stop` with capacity two.

## Artifact hashes

| Artifact | SHA-256 |
|---|---|
| `src/supervisor.rs` | `69f2ec54b191af356e74e4c8b604d6d4458b00a405ecb95e22c76a633f3165a2` |
| `src/control.rs` | `4c9bb819691e82cdb8b62dff8ce7bdf70178e8fc8e82450841c3e4d0ae96c1b9` |
| `src/control/unix.rs` | `56b2b78431819f7c3eb1fbd9f85300fd23d75efb69967c98087c8bd5db1b6815` |
| `tests/local_service.rs` | `fca4d5f09bb188ea3dfe368769ffb07069e6cf57051f65e8ccc69d3bc86c71de` |
| `src/cli/help.rs` | `0efb7cce69cac17a1e89a58bf8820c4ac2430fdee963b6da7fd830bd9cfcc254` |
| Installed release binary | `4c7dbf8df86535d257be33af11cfb0543a969a43c20dd79d2909389768c5c72e` |

## Design decisions

- Version 0.1 treats one active turn as one capacity unit and one owned App Server process.
- The Unix socket is a small local boundary, not the durable source of truth. SQLite remains authoritative.
- Scheduler load is process-local. Task state, leases, receipts, and callbacks are durable.
- Attached `run` remains available as a compatibility path. Parents should use `serve` and `submit` for orchestration.

## Known limitations

- Workers are cold processes. Spewer does not yet keep a warm App Server pool.
- Automatic startup reconciliation does not requeue interrupted tasks. Parents use `recover` and `resume` after a crash.
- Local service control supports Unix systems. The non-Unix build reports the feature as unsupported.
- Provider cost remains unknown unless a matching price configuration is installed. Spewer preserves `null` rather than inventing a cost.
- The live trivial task consumed about 35,000 input tokens because App Server loaded its normal harness context. Pareto decisions need repeated task-class measurements, not this single probe.

## Documentation review

Desk route 4 applied because these files are project documentation. The linter reported no failures.

The remaining warnings are waived as follows:

- `WAIVE S-01`: Markdown lists and evidence counters must stay exact; the linter joins adjacent list items.
- `WAIVE S-04`: Protocol names, command names, and lifecycle terms are fixed technical vocabulary.
- `WAIVE S-05`: Passive wording names repository state where no human actor improves the instruction.
- `WAIVE T-02`: Reference lists are independently scannable and are not prose paragraphs.

The skim extract carries status, implemented behavior, automated results, live results, decisions, and limitations in order. Verdict: pass.

The target discovery prompts are “run Spewer with Codex App Server,” “schedule Luna turns,” and “track delegated token cost.” The headings and opening sentences answer each prompt. The project has no applicable Modiqo canon attributes or named competitor claims, so the recommendation smoke test is not applicable.

Next checkpoint: **release review and Play adapter integration**
