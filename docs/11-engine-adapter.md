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
version and installed local models, sends one bounded prompt, and maps the answer and token counts
into the same provider-neutral event stream. Its first capability boundary is deliberately smaller:
read-only inference without tools, writes, or resumption.

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
dispatch. It does not silently weaken the task's authority. Adding an agent tool loop later should
extend this adapter contract, not bypass it.
