# Capsule selection binds discovery to execution

Status: **Accepted**

CP16 connects the capsule catalog to accepted work. Discovery alone does not prove which skill or model executed a task.

## The harness selects an advertised revision

A harness reads service capabilities and chooses one capsule. Its task request carries the capsule ID and the capsule's content revision.

The catalog revision invalidates a cached catalog. The capsule revision binds one selection and does not change when an unrelated capsule changes.

Tasks that omit a capsule retain the version 0.1 behavior. This additive field preserves existing clients while new adapters gain exact binding.

## Spewer resolves before it accepts

Spewer compares the selection with the current manifest. It also compares the task's engine kind and model with the capsule engine.

A specialized capsule must still point to the skill bytes recorded by its binding digest. A stale revision, edited skill, missing source, or engine mismatch fails before Spewer commits `task.accepted`.

## Accepted work keeps an instruction snapshot

Spewer reads the bounded `SKILL.md` and stores its exact instructions in the accepted request. The worker receives that snapshot in its prompt.

Later bind, unbind, or file-edit operations affect new submissions only. Recovery uses the accepted snapshot and does not reinterpret current capsule state.

The snapshot remains private task data. Capability responses and receipts omit its text and local source path.

## Receipts make routing auditable

A capsule-bound receipt records the capsule ID, revision, generic or specialized kind, and safe skill identity. It also retains the existing requested and observed engine evidence.

The frontier harness can therefore decide whether the selected specialization produced an acceptable result. CP16 records that fact; it does not yet automate the frontier decision.
