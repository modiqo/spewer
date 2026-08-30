# CP22 evidence: specialize a named Codex capsule

Status: **Complete**

Starting revision: `8ae31394adbc16d2e76fe3a05bf06de00df0e077`

Date: 2026-08-29

## A named Codex capsule preserves the generic default

`capsule add` now accepts `codex-app-server` after a successful Codex doctor check. The command
persists a separate generic capsule with the requested model.

The integration test adds `play-codex`, binds a fixture skill, and queries the running service.
The live catalog changes without a Spewer restart. The `default` capsule remains generic.

## Play installs and advertises its complete routing card

Play's targeted bootstrap updated Codex from Play 0.4.71 to 0.4.76. It verified the Play CLI, 31
runtime entrypoints, Rote 0.75.0, authentication, hooks, and the Codex skill roots.

The first binding exposed a parser defect. Play uses a folded YAML description, but Spewer stored
only the `>` marker. Spewer now parses folded and literal block descriptions before advertising a
skill. A unit test covers the folded form.

The corrected live card reports:

```text
capsule: play-codex
kind: specialized
engine: codex-app-server
model: gpt-5.6-luna
skill: play
skill digest: 833cf8a3b7538ea212e06f00b847932a94a394051c358614b6c2a76b2d3f1ac5
```

## The specialized capsule ran the Play CLI

This live command asked Codex behind Spewer to use Play's deterministic shortcut:

```console
$ spewer ask 'play cheat-sheet' --capsule play-codex --json
```

The answer contained the Play cheat sheet. The completed receipt recorded these identities:

```text
task: tsk_61fbabd9eb9eb895fc93ab59
receipt: rcp_8f38c0908f4c84aa865cc9ec
capsule: play-codex
capsule kind: specialized
skill: play
model: gpt-5.6-luna
tool calls: 1
wall time: 74,018 ms
changed files: 0
```

The receipt retained the same skill digest as the capability card. This match proves Spewer ran the
accepted specialization rather than an unbound worker.

## Capsule selection now activates the skill explicitly

The initial CP22 prompt copied the bound skill but did not state that capsule selection satisfied
the skill's activation gate. Spewer now adds a generic activation envelope before the immutable
instructions. It names the selected skill and tells the worker to enter it for that task.

This behavior applies to every specialized Codex or Ollama capsule. Spewer does not hard-code a
Play prefix. The integration test checks the activation envelope and the exact bound instructions.

## Automated and quality evidence passed

`cargo test --all-targets` passed 71 tests, including 47 library tests. Formatting, Clippy, and
Rustdoc passed with warnings denied.

Dependency policy, unused dependency analysis, line limits, panic checks, schema checks, and
`git diff --check` passed. `cargo deny` retained the known `windows-sys` duplicate and unused `Zlib`
allowance warnings.
