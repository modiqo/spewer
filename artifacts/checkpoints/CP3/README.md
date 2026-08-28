# CP3 evidence: durable event journal

CP3 makes accepted tasks and normalized events recoverable from SQLite. The parent CLI now uses the durable runner and exposes `status`, `tail`, and `rebuild` queries.

## Source and fixtures

- Starting revision: `13be5601ed99ac0dec14df01d84dd5f1eb67c0f1`
- Migration SHA-256: `a46f1e929b022eee6dbdc64c34446e5e52c7592583b7edd01bb4dfb67c55e96f`
- Restart fault test SHA-256: `f0bcbf860de84bc99305ab0e974c709d9017e796c424f38a366fd418b73f70fe`

## Verification

The following commands exited successfully on 2026-08-28:

- `cargo fmt --all`
- `cargo clippy --all-targets -- -D warnings`
- `cargo test --all-targets`: 14 tests passed
- `cargo doc --no-deps`
- `cargo deny check`: advisories, bans, licenses, and sources passed; three configured license allowances were unused
- `cargo machete`: no unused dependencies
- `./scripts/check-rust-source-lines.sh`
- `./scripts/check-panic-primitives.sh`
- `./scripts/check-doc-lines.sh`

The storage tests prove atomic acceptance, gap-free sequence assignment, source-event deduplication, byte-identical replay, restart persistence, and projection rebuild after deliberate corruption. Closing after a committed source event and retrying it after restart emits no second normalized event.

## Decisions and limitations

SQLite runs in WAL mode with full synchronous commits behind one bounded writer channel. Receipt delivery and Codex thread reconciliation remain out of scope until CP4 and CP5. CP3 uses a controlled restart at the commit boundary rather than an operating-system kill; the full kill matrix belongs to CP4.

Next checkpoint: CP4.
