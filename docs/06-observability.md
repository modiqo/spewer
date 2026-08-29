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

## Heartbeats detect silence without claiming failure

The supervisor emits `task.heartbeat` while the engine remains alive but produces no source event. The heartbeat includes process health and silence duration.

A silence threshold changes the projection to `stalled`. A separate policy decides whether to interrupt, retry, or escalate.

## Usage preserves provider facts and derived cost

Spewer stores provider-reported token categories without merging them. Missing categories remain `null`, not zero.

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
