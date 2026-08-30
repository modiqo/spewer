# CP23 evidence: observe Codex and Ollama activity safely

Status: **Complete**

Date: **2026-08-29**

## One command follows both worker engines

`spewer watch <task-id>` replays the durable event journal and follows new events until the task
becomes terminal. Its header identifies the accepted capsule, specialization, skill digest,
engine, and model. Detached `ask` output now returns the exact `watch` argument array.

The renderer omits high-volume provider deltas, unknown notifications, and standard error. It also
omits raw commands, arguments, tool output, and hidden reasoning. `spewer tail` retains the full
machine-readable structural record.

## Ollama reports liveness during silent inference

The local Qwen3 task `tsk_741c81c189a0665372323959` completed through `qwen3-local`. Its trace
identified engine `ollama` and model `qwen3:30b-a3b`. It committed eight one-second `model active`
heartbeats before the complete response arrived. The final trace reported 114 input tokens, 107
output tokens, and explicit `not-reported` labels for unavailable cached and reasoning counts.

The runner test uses a delayed inference future. It proves that a durable `task.heartbeat` with
`activity: model_active` appears before completion.

## Luna proves that the bound Play runtime executed

The completed task `tsk_9f263a5f0e0c876b93fc062c` ran through the specialized `play-codex`
capsule with `gpt-5.6-luna`. Its trace retained the Play revision and digest prefix
`833cf8a3b753`, then reported:

```text
tool started commandExecution recall
tool completed commandExecution recall
tool started commandExecution play-cheat-sheet
tool completed commandExecution play-cheat-sheet
done status=completed cursor=3613
```

The safe command label proves that Luna invoked the Play CLI. The trace does not contain its
arguments or output. The result used 92,324 input tokens and 67,840 cached input tokens. It also
used 4,056 output tokens and 372 reasoning tokens.

A broader request, `tsk_1603aa7f12fdc09632e75fda`, invoked `play-cheat-sheet` and continued for
more turns. It used only 4 of 20 allowed tool calls. Its reported input, output, and reasoning
tokens totaled 121,773 and exceeded the 100,000-token task budget. The minimal cheat-sheet request
above confirms a complete specialized run.

## Automated and quality gates passed

All 75 Rust tests passed across library, CLI, storage, recovery, service, contract, and restart
suites. Formatting, Clippy with warnings denied, and Rustdoc with warnings denied passed.

Dependency policy retained the known unused `Zlib` allowance and duplicate `windows-sys`
warnings. Unused dependency analysis, Rust and Markdown line limits, panic checks, Codex schema
checks, and `git diff --check` passed.
