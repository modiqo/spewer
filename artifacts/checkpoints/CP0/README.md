# CP0 evidence: contract and toolchain frozen

Status: **Passed**

Source revision: `edcc97b24d52db537d0b3777d135b4ff813bd5fe`

Date: 2026-08-28

## Frozen inputs

- Rust 1.96.1 with the minimal rustfmt and Clippy toolchain profile
- Tokio 1.53.1 with `rt`, `macros`, `process`, `io-util`, `sync`, `signal`, and `time`
- Rusqlite 0.40.2 with bundled SQLite
- Codex CLI 0.150.1
- 295 generated stable App Server schema files
- Aggregate schema SHA-256 `17d7491e8229234153c74e29a32db4eaed4f01ae0dfb1e90907f3efbe5ed695c`

## Commands and results

| Command | Exit | Result |
|---|---:|---|
| `cargo fmt --check` | 0 | Formatting passed |
| `cargo clippy --all-targets -- -D warnings` | 0 | Strict linting passed |
| `cargo test --locked` | 0 | One contract test passed |
| `RUSTDOCFLAGS="-D warnings" cargo doc --no-deps` | 0 | Documentation passed |
| `cargo deny check` | 0 | Advisories, licenses, bans, and sources passed |
| `cargo machete` | 0 | Frozen CP1 dependencies reported as not yet used |
| `sh scripts/check-rust-source-lines.sh` | 0 | Every handwritten Rust file is at most 500 lines |
| `sh scripts/check-panic-primitives.sh` | 0 | No forbidden panic primitive found |
| `sh scripts/check-doc-lines.sh` | 0 | Every maintained Markdown file is at most 500 lines |
| `sh scripts/check-codex-schema.sh` | 0 | Schema count and aggregate hash matched |
| `cargo build --timings` | 0 | Incremental build completed in 0.02 seconds |

## Decisions

ADR-0004 replaces the proposed TypeScript runtime with one Rust package. Tokio remains an outer driver around a synchronous deterministic core.

## Known limitations

The runtime dependencies are deliberately frozen but unused in CP0. CP1 must use the process, time, hashing, and CLI dependencies before its exit gate.

Next checkpoint: **CP1**
