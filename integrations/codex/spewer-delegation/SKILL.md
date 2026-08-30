---
name: spewer-delegation
description: Delegate bounded, independently checkable work to a configured Spewer capsule when a cheaper worker can execute while the frontier model retains judgment and the final response.
---

# Delegate bounded work through Spewer

Use Spewer when the task has a clear objective, explicit acceptance checks, bounded permissions, and useful work the frontier turn can do concurrently.

Keep ambiguous classification, user communication, tradeoffs, and the final answer in the frontier harness. A delegated task may pause at a typed human-input boundary as long as this harness remains available to relay the answer.

## Delegate

Create a complete Spewer task JSON with the smallest projected context and authority that can succeed. Use a stable idempotency key and callback consumer owned by this harness.

Read live capsule cards before choosing a worker:

```sh
spewer capabilities
```

Treat `network` and `tools` as hard routing limits. Do not delegate current or external-data work to a capsule with `network: false`. Do not delegate work that requires a tool unless the card lists that tool category. A specialized skill changes instructions; it does not grant network or tools.

Run:

```sh
spewer delegate <task.json> --capsule default
```

Store the returned task ID. Delegation performs live discovery and rejects a stale or missing capsule before acceptance. The structured response retains the catalog and capsule revisions for inspection.

## Check

Continue useful frontier work before polling. Then run:

```sh
spewer check <task-id>
```

When `ready` is false, wait at least `observation.poll_after_ms` before checking again. A durable native adapter may pass `--after <stored-event-cursor>` to avoid replaying earlier events.

When the projection status is `input_required`, inspect `projection.pending_input`. Ask the user the exact nonsecret question in this frontier conversation. Then return the answer to the same task with its unchanged request ID:

```sh
spewer respond <task-id> 99 --response '<typed-json-response>'
```

Pass the exact JSON request ID; use the shell argument `'"input-one"'` for a string ID. Build the response shape required by the pending method. For `item/tool/requestUserInput`, answer every question ID with `{"answers":{"<question-id>":{"answers":["<answer>"]}}}`. For an approval request, use `{"decision":"accept"}`, `decline`, or `cancel` only after asking the user.

Never put a password, API key, access token, cookie, or other credential in `spewer respond`. Ask the user to authenticate directly with the provider or its owning harness, then relay only a nonsecret completion or choice when the pending schema allows it. Spewer rejects secret prompts. If no answer arrives within 30 minutes, the task escalates and releases its worker; inspect the terminal receipt before retrying.

After a successful response, continue checking the same task ID. Do not create a replacement task: Spewer resumes the same worker turn and pauses its execution wall budget during the human wait.

When a receipt arrives, inspect its status, capsule, skill, requested and observed models, verification, artifacts, usage, and waiver. Verify the result against the user's actual request before using it.

The harness may acknowledge the message only after it durably stores and applies the receipt. This skill does not prove that host-specific boundary, so do not acknowledge automatically.

## Cancel

When delegated work is no longer wanted, run:

```sh
spewer cancel <task-id> --reason "<why the parent stopped it>"
```

Treat cancellation as idempotent. Retrieve and inspect its terminal receipt through `spewer check`.
