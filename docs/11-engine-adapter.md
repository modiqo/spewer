# One small adapter contract makes the harness replaceable

Status: **Accepted**

Spewer does not abstract an entire Claude, Codex, Kimi, Pi, or Qwen harness. It abstracts the narrow boundary needed to supervise one bounded run.

## The public boundary has three parts

An engine adapter exposes:

- capabilities, including models, usage reporting, and resumption
- one bounded task request
- a finite stream of provider-neutral source events with stable deduplication keys

The controller owns sequencing, projection, checkpoints, budgets, receipts, and callbacks. Provider-specific JSON-RPC types remain inside the adapter.

Codex App Server is the full agent adapter. It supplies process control, threads, turns, tools,
model discovery, usage notifications, and resumption. Spewer adds durable supervision around it
instead of recreating those features.

The second production adapter connects to a loopback Ollama server. It discovers the live Ollama
version and installed local models, sends a bounded prompt, and normalizes answers and token counts.

CP19 adds an optional `web_search` loop when the task allows network access and the Spewer process
has `OLLAMA_API_KEY`. The adapter still rejects commands, writes, approvals, and resumption.

## The fake engine proves the seam

The deterministic fake engine advertises `fake-local`, emits plans, tools, usage, pauses, failures, and duplicate source events. It passes the same public task, reducer, budget, and parent-callback contracts without importing a Codex type.

This proves that the Spewer core is replaceable at the adapter seam. CP18 then exercises that seam
with local Qwen3 through the production Ollama adapter. Kimi, Pi, and other worker adapters do not
exist yet.

## The local adapter stays narrow

The local-model server implements capability discovery and emits the same provider-neutral source
events. It may report zero provider charge only when a versioned price source says so; otherwise
cost stays unknown. Wall time and available token counts remain evidence in the receipt.

The adapter rejects unsupported command, write, resumption, and approval requirements before
dispatch. It does not silently weaken the task's authority. Its search loop uses the same finite
event contract instead of bypassing it.
