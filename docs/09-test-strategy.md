# Tests prove contracts, crashes, and authority boundaries

Status: **Accepted**

Spewer's highest risks appear between components. The test strategy emphasizes recorded streams, transaction boundaries, process failure, and duplicate delivery.

## Four test layers divide responsibility

### Unit tests prove deterministic logic

Unit tests cover schema validation, state transitions, budget calculations, cost derivation, redaction, and event mapping. They use no live model.

Every state transition test names its input event, prior state, next state, and emitted effects.

### Contract tests prove adapter compatibility

Contract tests run recorded Codex streams through the normalizer. Fixtures include the Codex version, generated schema hash, source payload hash, and redaction record.

Each supported Codex version must pass the same normalized event expectations. An unknown notification must remain observable.

### Integration tests prove storage and processes

Integration tests start a real SQLite database and a deterministic fake engine process. They test migrations, transactions, restart recovery, and process cleanup.

These tests control process timing through explicit barriers. They do not depend on arbitrary sleeps.

### Live tests detect upstream drift

Live tests run a minimal task against an installed Codex App Server. They verify handshake, model discovery, thread creation, event parsing, and clean shutdown.

Live tests are optional for local development and required on a scheduled compatibility job. They must use a bounded fixture workspace and budget.

## The kill matrix covers every durable boundary

Fault injection can terminate Spewer at these points:

| Kill point | Required recovery result |
|---|---|
| Before task insert | No task exists |
| After task insert | Task remains `queued` |
| After engine start | Recovery finds or closes the engine |
| After source read, before event commit | Source event can arrive again |
| After event commit, before source acknowledgement | Duplicate source event has no second effect |
| After workspace change | Diff validation detects the change |
| After side-effect start | Recovery marks uncertainty or verifies the effect |
| After turn completion | Recovery builds the same terminal projection |
| After receipt commit | Receipt remains queryable |
| After callback send, before acknowledgement | Parent receives the same receipt again |

Every row must have one automated test before CP5 passes.

## Property tests protect state-machine invariants

Generated event sequences must preserve these properties:

- event sequence numbers increase without gaps after commit
- terminal task states never return to running
- used budgets never decrease within one attempt
- one source deduplication key creates at most one event
- one effect key reaches `verified` at most once
- projection replay equals incremental projection

The generator may include duplicate, delayed, unknown, and malformed source events.

## Security tests attempt authority expansion

Security fixtures attempt parent-path traversal, symlink escape, environment leakage, stale approval replay, and command-policy bypass. The expected result is rejection.

Redaction tests seed unique secret markers into every input channel. No persisted event, receipt, log, or artifact metadata may contain those markers.

## Compatibility tests freeze public examples

Every JSON example in [the task protocol](03-task-protocol.md) becomes a fixture. CI validates those fixtures against the current schemas.

Minor-version readers must accept earlier minor-version fixtures. Breaking fixture changes require a major protocol version or a migration.

## Evidence packets make checkpoint claims auditable

The checkpoint command records exact commands, exit codes, test counts, skipped tests, and artifact hashes. A green summary without the underlying packet does not pass.

Live model output remains nondeterministic. Live tests assert protocol and safety behavior rather than exact prose or hidden reasoning.
