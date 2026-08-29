# ADR-0009: expose three frontier actions over one reusable client

Status: **Accepted**

## Context

The Spewer service has eight lifecycle operations. Showing every operation to a frontier model increases tool-selection cost and pushes cursor and delivery mechanics into prompts.

## Decision

Spewer provides a reusable harness client with discovery, capsule-bound delegation, combined checking, and cancellation.

Frontier models see three actions: delegate, check, and cancel. The adapter performs capability lookup inside delegation and combines observation with result retrieval inside check.

The reference integration ships as an Agent Skill backed by CLI projections. Host-specific adapters may call the same Rust client directly.

## Consequences

Models do not manage socket paths, catalog revisions, event replay calls, or separate result probes during ordinary use. Structured output still exposes the identities that deterministic host code must store.

The generic client does not make a host continuation durable. Each harness retains that responsibility, and its adapter must acknowledge only after durable receipt application.
