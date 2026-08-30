# CP25 continues one task through typed human input

Status: **Complete**

CP25 relays nonsecret answers and approvals to delegated Codex workers. It does not create a replacement task.

## Product behavior

- App Server requests become durable `input.required` events and an `input_required` projection.

- `projection.pending_input` preserves the native request ID, method, and typed request shape.

- `spewer respond <task-id> <request-id> --response '<json>'` validates the exact pending request.

- Spewer commits `input.resolved` before sending the response to the same App Server worker turn.

- The task returns to `running`; the caller keeps checking the original task ID.

- Password, token, API-key, cookie, and other secret prompts are rejected. Authentication remains out of band.

- Human wait time pauses the task wall budget.

- No response within 30 minutes records `task.stalled`, escalates the task, closes the worker, and produces a terminal receipt without guessing.

The first implementation keeps the active App Server process and one worker slot while input is pending. A service crash during that wait remains an uncertain-execution boundary and escalates safely; live cross-restart input continuation is not claimed.

## Verification

The automated suite proves:

- typed request validation, exact request identity, and secret rejection;

- supervisor transition from `input_required` through `input.resolved` to completion;

- timer expiry without a fabricated answer;

- local control capability advertisement with `respond` and `input_timeout_seconds: 1800`;

- CLI parsing and help for typed responses;

- a fake App Server requesting a rideshare date range, receiving the JSON-RPC response, and completing the same task and thread;

- bundled frontier-skill installation and upgrade safety.

The checkpoint passed formatting, Clippy, all Rust targets, and Rustdoc warnings. It also passed dependency policy, line limits, panic checks, schema checks, and `git diff --check`.

## A live Play crossed three human boundaries

Task `tsk_09d0be3f7752d6d08bd634a5` stayed on one App Server thread. It requested a start date,
an exclusive end date, and approval to pull and run `modiqo/retrieve-rideshare-receipts@0.1.8`.
Each `spewer respond` resumed that task and Luna thread.

After approval, the Play opened the scoped Gmail OAuth browser from Luna. Authentication material
stayed with the provider-owned adapter. The verified result returned 23 receipts totaling
$1,498.46, then yielded a separate optional recurrence choice.

The current usage snapshot records 490,100 input tokens, including 446,208 cached input tokens. It
also records 5,388 output tokens, 2,285 reasoning tokens, and 10 tool calls. This is execution
evidence, not a Pareto comparison. This run has no price configuration or comparable
acceptance-score pair.
