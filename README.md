# Spewer

Spewer delegates bounded agent work to a cheaper model and returns a typed, verifiable receipt. The first engine uses Codex App Server.

This folder holds implementation code. The design and delivery gates live in [spewer-docs](../spewer-docs/README.md).

## Implementation status

CP0 through CP9 are implemented. The crate includes the Codex adapter, durable event journal, checkpoints, outbox callbacks, budgets, cost exports, Play-facing parent helpers, and a deterministic second engine. Evidence for every gate lives under `artifacts/checkpoints/`.

## Source layout

```text
src/
  cli.rs                command parsing and terminal output
  protocol.rs           engine-neutral public types
  reducer.rs            deterministic task state machine
  codex/                App Server transport and event mapper
  store/                event log, projections, checkpoints, and outbox
  workspace.rs          worktree lifecycle and artifact inventory
  telemetry.rs          usage, cost, and quality measurements
  budget.rs             deterministic hard-limit evaluation
  security.rs           redaction, approvals, and effect policy
  engine.rs             provider-neutral adapter contract
  fake.rs               deterministic second engine
  parent.rs             parent cursor and receipt application
tests/
  fixtures/             recorded engine streams and task fixtures
  *.rs                  contract, recovery, fault, and portability tests
```

Every handwritten Rust file is limited to 500 physical lines. The package forbids unsafe code, panic primitives, unchecked indexing, and unchecked arithmetic.
