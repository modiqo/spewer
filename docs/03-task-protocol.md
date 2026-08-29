# The public protocol contains tasks, events, and receipts

Status: **Accepted**

Spewer exposes an engine-neutral protocol. Provider fields appear only inside namespaced metadata.

## Protocol versions use additive compatibility

Every top-level object carries `protocol_version`. Version 0.1 accepts unknown optional fields and rejects unknown required semantics.

Minor versions add optional fields or event types. Major versions may change required fields, state transitions, or meanings.

## A task request declares work and limits

```json
{
  "protocol_version": "0.1",
  "task_id": "optional-client-id",
  "idempotency_key": "play:abc:step:4",
  "objective": "Update the parser and its focused tests",
  "acceptance": [
    "The focused parser tests pass",
    "No file outside src/parser and tests/parser changes"
  ],
  "workspace": {
    "path": "/absolute/project/path",
    "base_revision": "git-sha"
  },
  "context": {
    "files": ["src/parser/index.ts"],
    "notes": ["Preserve the public API"]
  },
  "permissions": {
    "filesystem": "workspace-write",
    "network": "deny",
    "commands": "allowlist"
  },
  "budgets": {
    "wall_seconds": 900,
    "tokens": 50000,
    "tool_calls": 100,
    "retries": 1,
    "cost_usd": 2.00
  },
  "engine": {
    "kind": "codex-app-server",
    "model": "configured-cheap-model"
  },
  "capsule": {
    "id": "default",
    "revision": "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
  },
  "callback": {
    "mode": "stream",
    "consumer_id": "play"
  }
}
```

The controller rejects a request before creating a worker when required fields fail validation. It records an accepted request before starting an engine.

`capsule` is optional for version 0.1 compatibility. New routing adapters select the ID and revision returned by service capabilities.

Spewer replaces any caller-supplied binding snapshot before acceptance. The accepted request stores the resolved skill instructions for execution and recovery.

## A task handle makes detachment safe

```json
{
  "protocol_version": "0.1",
  "task_id": "tsk_01J...",
  "status": "queued",
  "event_cursor": 1,
  "created_at": "2026-08-28T22:00:00Z"
}
```

The parent can disconnect after receiving this handle. It reconnects with `task_id` and its last acknowledged cursor.

## Normalized events use one envelope

```json
{
  "protocol_version": "0.1",
  "task_id": "tsk_01J...",
  "attempt": 1,
  "seq": 42,
  "type": "item.completed",
  "observed_at": "2026-08-28T22:01:10Z",
  "data": {},
  "source": {
    "engine": "codex-app-server",
    "method": "item/completed",
    "thread_id": "thr_123",
    "turn_id": "turn_456",
    "item_id": "item_789",
    "payload_hash": "sha256:..."
  }
}
```

`seq` increases once per task. Consumers acknowledge the highest contiguous sequence they processed.

## A receipt closes one attempt

```json
{
  "protocol_version": "0.1",
  "receipt_id": "rcp_01J...",
  "task_id": "tsk_01J...",
  "attempt": 1,
  "status": "completed",
  "summary": "Updated the parser and added three focused tests.",
  "artifacts": [
    {"kind": "git-diff", "uri": "artifact://...", "sha256": "..."}
  ],
  "verification": [
    {"command": "npm test -- parser", "exit_code": 0, "output_sha256": "..."}
  ],
  "usage": {
    "input_tokens": 0,
    "cached_input_tokens": 0,
    "output_tokens": 0,
    "reasoning_tokens": 0,
    "wall_ms": 0,
    "tool_calls": 0,
    "actual_cost_usd": null
  },
  "engine": {
    "kind": "codex-app-server",
    "requested_model": "configured-cheap-model",
    "observed_models": []
  },
  "capsule": {
    "id": "default",
    "revision": "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
    "kind": "specialized",
    "skill": {
      "name": "parser-update",
      "description": "Update and verify bounded parser changes",
      "revision": "1",
      "digest": "abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789"
    }
  },
  "final_event_seq": 99,
  "completed_at": "2026-08-28T22:02:00Z"
}
```

Statuses are `completed`, `failed`, `cancelled`, or `escalated`. A completed receipt without acceptance evidence must include an explicit verification waiver.

## Delivery is at least once

Spewer can deliver the same receipt more than once. The stable `receipt_id` and task idempotency key let the parent apply it once.

The parent acknowledges a receipt after storing or applying it. Spewer retains the outbox row until that acknowledgement commits.
