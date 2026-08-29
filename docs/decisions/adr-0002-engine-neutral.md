# ADR-0002: Spewer's public protocol stays engine-neutral

Status: **Accepted**

Date: 2026-08-28

## Context

Codex App Server offers the cleanest first engine. Spewer must later support other harnesses and cheaper local models.

Copying Codex request and event types into Spewer's public API would bind every future engine to upstream details. It would also leak account and transport behavior into parent integrations.

## Decision

Spewer defines stable task, event, checkpoint, capability, and receipt contracts. Each engine adapter translates its native protocol at the boundary.

Provider fields may appear only inside namespaced engine metadata. Core state transitions cannot depend on a provider-specific field.

## Consequences

Adapters perform more translation. The core gains portability, recorded-fixture testing, and consistent parent behavior.

Some engines cannot provide every capability. Capability negotiation must reject the task or select an explicit fallback.

## Rejected alternatives

Making every engine emulate the complete Codex protocol would preserve one adapter shape. It would also turn Codex-specific behavior into accidental Spewer policy.

Using only the lowest common denominator would hide useful plans, usage, diffs, and approvals. Spewer instead normalizes common facts and retains namespaced source metadata.

## Review trigger

Review this decision if CP9 requires a public task change to add the fake second engine.
