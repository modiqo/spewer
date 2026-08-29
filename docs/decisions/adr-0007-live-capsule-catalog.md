# ADR-0007: keep capsule manifests durable and capability lookup live

Status: **Accepted**

## Context

Frontier harnesses need to distinguish a generic worker from one specialized by a bound skill. Rebuilding an adapter for every binding would make setup brittle, while storing bindings only in a running service would lose them on restart.

## Decision

Spewer stores owner-private capsule manifests outside the event database. A capsule remains the stable worker identity; an optional skill binding changes its advertised kind from `generic` to `specialized`.

The existing capability operation reads manifests at request time and returns a content-addressed catalog revision. Harness adapters are generated against the stable service protocol and discover current capsules dynamically.

Skill bindings record a canonical source, but capability advertisements expose only the skill identity, description, revision, and digest.

## Consequences

Binding and unbinding become visible without a service restart or adapter regeneration. Restarting the service preserves the same advertisements and capability revision.

The filesystem catalog needs atomic writes and owner-only permissions. Task-to-capsule binding remains a separate protocol checkpoint so discovery can ship without weakening current task acceptance guarantees.
