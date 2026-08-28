# CP1 evidence: App Server process control

Status: **Passed**

Source revision: `bb00b1260588649d3fe4bf386be88e073ca0ba88`

Date: 2026-08-28

## Implemented behavior

- One owned App Server process in an isolated process group
- Explicit child environment inheritance
- Bounded command and event channels
- Correlated JSON-RPC requests and notifications
- Required `initialize` and `initialized` handshake
- Typed request, startup, and shutdown deadlines
- Graceful group termination with forced-kill fallback
- Observable malformed lines, unknown notifications, stderr, and exits

## Commands and results

| Command | Exit | Result |
|---|---:|---|
| `cargo run --locked -- doctor --engine codex` | 0 | Codex CLI 0.150.1 reported ready |
| `cargo clippy --all-targets -- -D warnings` | 0 | Strict linting passed |
| `cargo test --locked` | 0 | Six tests passed |
| `RUSTDOCFLAGS="-D warnings" cargo doc --no-deps` | 0 | Documentation passed |
| `cargo deny check` | 0 | Dependency policy passed |
| `cargo machete` | 0 | Only CP2 hashing and time dependencies remain unused |
| `sh scripts/check-rust-source-lines.sh` | 0 | Source-size gate passed |
| `sh scripts/check-panic-primitives.sh` | 0 | Panic-primitive gate passed |

## Live result

The installed App Server returned its Codex home, `unix` platform family, `macos` platform, and Spewer user agent. The doctor command completed a model-list request without creating a thread.

## Known limitations

CP1 does not create tasks or map engine events. Those behaviors belong to CP2.

Next checkpoint: **CP2**
