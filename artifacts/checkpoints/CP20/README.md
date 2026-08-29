# CP20 evidence: capsule-aware plain ask

Status: **Complete**

Starting revision: CP19 worktree based on `ab3d597592125bc3affcbcff3078863cc0f81b05`

Date: 2026-08-29

## Implemented behavior

- Local configuration stores the capsule selected when `--capsule` is absent.
- Existing version 1 files load `default` when the new field is absent.
- `spewer capsule default <id>` validates and persists an installed capsule atomically.
- `spewer capsule show [<id>]` reports selection, ask arguments, available web authority, output
  choices, and the detached-service capability command.
- Plain attached `spewer ask` prints answer text and telemetry. `--json` retains structured output.
- `--web` remains an explicit per-task network grant.

## Compatibility evidence

The configuration test removes `default_capsule` from a serialized version 1 file. Loading restores
the Luna capsule ID, and an atomic update persists `qwen3-local` without weakening file privacy.

The install and capsule integration test runs `capsule show` and `capsule default`. The local
service test proves that plain ask binds the `default` capsule and that `ask --json` returns its
receipt evidence.

## Quality evidence

`cargo test --all-targets` passed 70 tests. The library set contains 46 tests.

Formatting and Clippy passed with warnings denied. Rustdoc passed with warnings denied. Dependency
advisory, license, ban, source, and unused-dependency policies passed. Source and documentation line
limits, the panic primitive audit, Codex schema verification, documentation lint, and
`git diff --check` also passed.

`cargo deny` retains the known duplicate `windows-sys` warning and unused `Zlib` allowance. Neither
warning changes CP20 behavior.
