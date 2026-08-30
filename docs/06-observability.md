# Spewer measures progress, cost, and verified quality

Status: **Accepted**

Spewer exposes enough evidence to compare cheaper workers with frontier execution. It does not reduce that comparison to one intelligence score.

## Progress describes observable work

Every task projection exposes these fields:

| Field | Meaning |
|---|---|
| `status` | Current task state |
| `phase` | Starting, acting, verifying, waiting, or delivering |
| `plan` | Explicit engine plan when available |
| `active_item` | Current command, tool, change, or response item |
| `last_activity_at` | Time of the latest accepted event |
| `elapsed_ms` | Wall time since attempt start |
| `usage` | Current token and tool counters |
| `budget_remaining` | Remaining declared limits |
| `workspace_diff` | Current diff hash and changed-file count |

Spewer reports `completed_steps / total_steps` only for an explicit plan. It otherwise omits percentage completion.

## Watch exposes activity without exposing reasoning

`spewer watch <task-id>` follows the durable journal and identifies the accepted capsule, skill,
engine, and model. It renders safe Codex tool labels and filters high-volume deltas. It also filters
standard error, raw commands, arguments, tool output, and unknown provider notifications.

The Ollama runner emits `task.heartbeat` once per second while a complete local response is still
pending. The heartbeat reports `activity: model_active` and elapsed time. It proves liveness but
does not claim percentage progress or reveal model reasoning. Codex normally supplies structural
item events instead, including reasoning start and completion boundaries without their contents.

Spewer does not turn model-heartbeat silence into `stalled` automatically. That policy remains
separate so liveness evidence cannot silently become retry or cancellation authority. The one
explicit stall boundary is a Codex task waiting 30 minutes for typed human input; it records
`task.stalled`, escalates without guessing, and releases the worker.

## Usage preserves provider facts and derived cost

Spewer stores provider-reported token categories without merging them. Missing categories remain `null`, not zero.

The Codex token budget uses cumulative provider usage across a turn. Every model continuation after
a tool call can add the repeated prompt context again. Provider-reported cached input remains a
separate counter. The 1,000,000-token ask default bounds cumulative work; it does not enlarge the
model context window.

Cost uses a versioned price configuration with effective dates. Every derived cost record names its price source and configuration hash.

Local models can report zero provider charge while still recording wall time, CPU time, GPU time, memory peaks, and energy estimates when available.

## Model identity includes reroutes

Every attempt records the configured model, requested model, and observed model sequence. A Codex `model/rerouted` event changes observed history without rewriting the request.

Reports group results by observed model and engine version. This prevents silent reroutes from polluting comparisons.

## Quality comes from task-specific verification

Each task declares acceptance criteria before dispatch. Verification emits structured results with commands, exit codes, checks, and artifact hashes.

Quality records may include:

- acceptance checks passed and attempted
- test commands and exit codes
- lint or type-check results
- human acceptance or rejection
- frontier review outcome
- retry and escalation counts
- regressions found after delivery

A receipt cannot infer quality from model confidence. It uses acceptance evidence or an explicit waiver.

## Pareto IQ compares equivalent task classes

Pareto IQ plots verified quality against actual cost for comparable tasks. It can also show a counterfactual frontier cost when the comparison method is recorded.

Every comparison names the task class, sample count, verification method, engine versions, models, and price configuration. A single anecdotal run remains an example.

## Logs protect secrets by default

Normalized events store structural facts and hashes. Raw prompts, reasoning blocks, command output, and tool results follow a configurable redaction policy.

The default UI shows reasoning summaries only when the engine supplies them and policy permits them. Spewer never requires hidden chain-of-thought for progress.
