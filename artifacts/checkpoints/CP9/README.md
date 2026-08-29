# CP9 evidence: replaceable engine seam

Starting revision: `a57dbb538df8076370197b0877c7b7d295268978`.

The public engine contract contains capabilities, one bounded task, and provider-neutral source events. Unsupported models and resumption fail before dispatch.

The contract SHA-256 is `0711ef68342932d3bdae103de413c2248e3dc17f3fb966a73ca608a4fb6d2cc1`. The fake adapter SHA-256 is `e32ef5cc83e4df96481f2c0c63b3883480af12239744922805660c8f1882afe9`.

The deterministic fake engine emits plans, tools, usage, pauses, failures, completion, and duplicate source events. It runs the same public task, reducer, budget, receipt, and callback contracts without a Codex wire type. Capability negotiation makes unsupported behavior explicit.

All 25 tests and every repository gate passed. The separate engine design describes the narrow work needed for a future Kimi, Qwen, Pi, or local-model adapter.

Next checkpoint: release 0.1 review.
