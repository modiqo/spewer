# One small adapter contract makes the harness replaceable

Status: **Accepted**

Spewer does not abstract an entire Claude, Codex, Kimi, Pi, or Qwen harness. It abstracts the narrow boundary needed to supervise one bounded run.

## The public boundary has three parts

An engine adapter exposes:

- capabilities, including models, usage reporting, and resumption
- one bounded task request
- a finite stream of provider-neutral source events with stable deduplication keys

The controller owns sequencing, projection, checkpoints, budgets, receipts, and callbacks. Provider-specific JSON-RPC types remain inside the adapter.

Codex App Server is the production adapter because it already supplies process control, threads, turns, tools, model discovery, usage notifications, and resumption. Spewer adds durable supervision around it instead of recreating those features.

## The fake engine proves the seam

The deterministic fake engine advertises `fake-local`, emits plans, tools, usage, pauses, failures, and duplicate source events. It passes the same public task, reducer, budget, and parent-callback contracts without importing a Codex type.

This proves that the Spewer core is replaceable at the adapter seam. It does not claim that a Kimi, Qwen, Pi, or local-model adapter already exists.

## A future local adapter stays narrow

A local-model server needs to implement capability discovery and emit the same provider-neutral source events. It may report zero provider charge while retaining wall time and resource facts. Missing token categories remain unknown.

The adapter must reject unsupported sandbox, resumption, approval, or usage requirements before dispatch. It may not silently weaken the task’s authority.
