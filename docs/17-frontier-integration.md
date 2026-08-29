# Frontier integration exposes three task actions

Status: **Accepted**

CP17 packages the existing service lifecycle for frontier harnesses. It does not move classification or final judgment into Spewer.

## The reusable client hides transport mechanics

The harness client exposes discovery, delegation, checking, and cancellation. Delegation performs live capability lookup and binds the chosen capsule revision before submission.

Checking combines cursor-based observation with non-consuming result retrieval. Its `ready` field states whether a stable terminal message exists. A simple CLI caller can replay from zero; a durable adapter stores the returned cursor. Both store the terminal message before acknowledging delivery.

The client does not own a harness continuation. Each host still stores its private continuation and applies a receipt exactly once.

## Models see three actions

The model-facing actions are `delegate`, `check`, and `cancel`. Discovery happens inside delegation for the selected capsule ID.

`spewer delegate` accepts a complete task request and replaces its capsule and engine fields from current capabilities. `spewer check` returns observation and result in one JSON object. The existing cancel command remains idempotent.

## The reference skill teaches the ownership boundary

The `spewer-delegation` skill applies to bounded, checkable work that can run independently. It tells the frontier model to retain ambiguous judgment, user communication, and the final answer.

The skill uses the CLI projection so Codex can exercise the same client contract without a custom in-process extension. Other harnesses can call the Rust client directly or project the same operations into native tools.

## Installation preserves user changes

`spewer install` places the reference skill under the configured Codex home. It treats identical content as already installed.

Spewer refuses to overwrite different content at the same path. The user must review or remove that conflict explicitly.

## Host durability remains host-specific

The client returns every identity needed by a durable adapter. It cannot persist a foreign harness continuation because Spewer does not own that state.

Play remains the first adapter that proves the complete prepared, submitted, claimed, applied, and acknowledged inbox lifecycle.
