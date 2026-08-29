# CP21 evidence: resolve Ollama's implicit latest tag

Status: **Complete**

Starting revision: CP20 worktree based on `ab3d597592125bc3affcbcff3078863cc0f81b05`

Date: 2026-08-29

## The live failure identified an exact-name mismatch

The user pulled `mistral`. Ollama listed the installed model as `mistral:latest`, while Spewer
rejected the shorter name as missing. Both the installed binary and development doctor returned the
same canonical Ollama list, which isolated the failure to Spewer's literal comparison.

## Resolution preserves canonical identity

Discovery now checks an exact name first. An untagged final path component also checks the
corresponding `:latest` name. Explicit tags still require exact matches.

`capsule add` stores the resolved installed name. Direct tasks can retain an accepted shorthand,
while capability negotiation keeps the canonical model visible.

## Live evidence

The live doctor resolved `mistral` to `mistral:latest`. A temporary owner-private catalog then ran:

```console
$ spewer capsule add mistral-local --engine ollama --model mistral
```

The resulting manifest and public card both stored `engine.model` as `mistral:latest`. The temporary
catalog was removed after inspection; the user's catalog was not changed by the test.

## Automated evidence

A recorded doctor fixture now proves that `mistral` resolves against an installed
`mistral:latest`. The existing missing-model test still proves actionable rejection.

`cargo test --all-targets` passed 71 tests, including 47 library tests. Formatting and Clippy
passed with warnings denied. Rustdoc, dependency policy, unused-dependency analysis, source and
documentation line limits, the panic audit, Codex schema verification, and `git diff --check` also
passed.

`cargo deny` retains only the known duplicate `windows-sys` warning and unused `Zlib` allowance.
