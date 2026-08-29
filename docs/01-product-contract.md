# Spewer delegates bounded work without surrendering parent control

Status: **Accepted**

Spewer is an independent, open-source supervisor for cheaper agent workers. A parent harness delegates one bounded task and receives one typed receipt.

## The parent remains responsible for judgment

The parent harness decides whether to delegate. It owns the user conversation, frontier planning, private context, and final response.

Spewer owns the delegated run. It controls the worker process, projected context, permissions, budgets, progress records, recovery, verification, and result delivery.

Play can classify reusable work and create a typed handoff. Play retains its control flow and private continuation after Spewer returns.

## Version 0.1 proves one narrow path

Version 0.1 delegates a local repository task through Codex App Server. The selected Codex model should come from configuration and model discovery.

The first release must prove these outcomes:

1. A parent submits a bounded task and receives a stable task identifier.
2. Spewer starts Codex App Server and records its thread identifier.
3. Spewer exposes progress from plans, items, tools, diffs, and usage.
4. Spewer survives its own restart without losing acknowledged work.
5. Spewer returns a typed receipt with artifacts, evidence, cost, and status.
6. A repeated callback cannot apply the same result twice.

## The first release excludes broad orchestration

Version 0.1 does not provide a chat interface, global memory, or frontier planning. It does not coordinate arbitrary agent graphs.

Version 0.1 also excludes remote multi-tenant hosting. Every run belongs to one local user and one declared workspace.

Spewer does not expose hidden reasoning as progress. It records observable events and optional reasoning summaries when policy permits them.

## Five invariants constrain every implementation

1. **The task protocol remains engine-neutral.** Codex fields stay inside the Codex adapter or typed engine metadata.
2. **Every accepted event is durable.** A process crash cannot erase an event after Spewer acknowledges it.
3. **Every side effect has an idempotency key.** Recovery cannot repeat an effect silently.
4. **Progress never invents certainty.** Spewer reports a percentage only when an explicit denominator exists.
5. **A receipt carries evidence.** A successful status without verification evidence is incomplete.

## Success requires measured quality and cost

Spewer records tokens, elapsed time, tool activity, retries, model identity, and actual cost inputs. It also records verification outcomes and escalation.

Pareto IQ is a comparison, not a synthetic intelligence score. Reports plot verified quality against cost for comparable task classes.

## The second engine tests the abstraction

Codex App Server supplies the first complete engine contract. A later provider-neutral engine server can drive Kimi, Qwen, or local models.

The second engine must implement Spewer's adapter interface without changing the task or receipt schemas. Any required schema change must pass the compatibility process in [the task protocol](03-task-protocol.md).
