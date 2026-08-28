# CP2 evidence: one bounded Luna run

Status: **Passed**

Source revision: `0244ecda552789e72b4211e557e07dd80ade42b6`

Date: 2026-08-28

## Implemented behavior

- Model discovery and explicit availability rejection
- Detached Git worktree creation at a pinned base commit
- Codex `thread/start` and `turn/start` dispatch
- Engine-neutral plans, items, deltas, usage, reroutes, diffs, and terminal events
- Deterministic in-memory projection
- Allowed-path enforcement and a content-addressed binary diff
- Typed receipt with requested and observed model identity
- `spewer run <task.json> --engine codex` JSONL output

## Automated results

| Command | Exit | Result |
|---|---:|---|
| `cargo clippy --all-targets -- -D warnings` | 0 | Strict linting passed |
| `cargo test --locked` | 0 | Eleven tests passed |
| `RUSTDOCFLAGS="-D warnings" cargo doc --no-deps` | 0 | Documentation passed |
| `cargo deny check` | 0 | Dependency policy passed |
| `cargo machete` | 0 | No unused dependency found |
| Source-size, panic, document, and schema scripts | 0 | All local gates passed |

## Live Luna result

Spewer discovered `gpt-5.6-luna` through App Server and ran one isolated task. Luna created only `result.txt`, with the exact requested line.

The run completed in 13.599 seconds. Spewer recorded 184 normalized events, three tool calls, 54,121 input tokens, 37,120 cached input tokens, 448 output tokens, and 90 reasoning tokens.

The receipt recorded `gpt-5.6-luna` as both requested and observed. Its binary diff hash was `40384e5a6e25fd12f179e614f51481047c74a3009e450e555596bb73cdfae03c`.

The first live attempt exposed a wire-format mismatch between prose documentation and the generated schema. The pinned schema requires `workspace-write` for `thread/start.sandbox`; the adapter now follows that generated contract.

## Known limitations

Events remain in memory at CP2. CP3 moves acceptance and event ingestion into SQLite transactions.

Next checkpoint: **CP3**
