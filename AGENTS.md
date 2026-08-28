# Spewer implementation rules

Read [the design index](../spewer-docs/README.md) before changing production code. Work only inside the active checkpoint from [the implementation plan](../spewer-docs/08-IMPLEMENTATION-CHECKPOINTS.md).

## Preserve the architecture boundaries

- Keep Codex protocol types inside `src/engines/codex`.
- Keep public tasks, events, checkpoints, and receipts engine-neutral.
- Append an event before exposing its state change.
- Treat file state, Codex thread state, and Spewer state as separate records.
- Use stable idempotency keys for callbacks and external effects.
- Never report percentage progress without an explicit denominator.
- Never require hidden reasoning for progress or recovery.

## Finish one checkpoint before starting another

Run every acceptance test named by the active checkpoint. Store its evidence packet under `artifacts/checkpoints/CP<N>/`.

Update the relevant design document when implementation disproves an assumption. Add or supersede an ADR when the change affects a public contract.

Do not mark a checkpoint complete when you skip a required live test. Record the skip and leave the checkpoint open.
