# CP13 evidence: harness service boundary

Status: **Passed**

Starting revision: `a313ac90e9ff3502249f126ea36c13f3dad3f99d`

Date: 2026-08-29

## Implemented behavior

- The local service exposes `capabilities`, `submit`, `observe`, `result`, `cancel`, `acknowledge`, `load`, and `stop`.

- Each operation uses the service-owned scheduler or database writer.

- `observe` returns one projection, gap-free events after the caller's cursor, and the next committed cursor.

- `result` reports readiness without consuming the terminal outbox message.

- Acknowledgement does not make the terminal result unavailable.

- Cancellation removes queued work before dispatch.

- Cancellation aborts active work, terminates the App Server process group, and commits one terminal receipt.

- Completion and cancellation use the durable terminal state to settle races.

- Repeated cancellation and acknowledgement are idempotent.

- Service errors preserve their stable error category across the control socket.

- Detached `ask` returns argument arrays for the canonical `observe`, `result`, and `cancel` commands.

- CLI help teaches the complete harness sequence and the next safe step for every new operation.

## Automated results

| Command | Exit | Result |
|---|---:|---|
| `cargo fmt --all -- --check` | 0 | Formatting passed |
| `cargo clippy --all-targets --all-features -- -D warnings` | 0 | Strict linting passed |
| `cargo test --all-targets` | 0 | 43 tests passed |
| `RUSTDOCFLAGS="-D warnings" cargo doc --locked --no-deps` | 0 | Public documentation passed |
| `cargo deny check` | 0 | Advisory, license, ban, and source policy passed |
| `cargo machete` | 0 | No unused dependency found |
| Repository source, panic, document, and schema scripts | 0 | All local gates passed |
| `git diff --check` | 0 | Patch hygiene passed |

The service tests cover capability negotiation, cursor observation, not-ready results, retained results, acknowledgement, and active process cancellation. Supervisor tests cover queue capacity, queued cancellation, running cancellation, terminal races, and repeated cancellation.

## Live installed result

An isolated service used the installed Codex App Server and this detached request:

```text
spewer ask "Return only the integer result of 6 multiplied by 7." --detach
```

Task `tsk_091a536b673984deb506ab75` completed with answer `42`. Observation reached event cursor 59. Result retrieval reported `ready: true` and `completed`.

The receipt requested and observed `gpt-5.6-luna`. It recorded 16,883 input tokens, 9,984 cached input tokens, 37 output tokens, 30 reasoning tokens, zero tool calls, and 4,145 milliseconds.

Acknowledging message `msg_949637cd8f13be4aa224aa70` returned `applied: true`. A later result lookup returned the same message identifier and completed state. `spewer stop` drained the service.

Provider cost remained unknown because the isolated run had no matching price configuration. The temporary live workspace was moved to Trash after shutdown.

## Artifact hashes

| Artifact | SHA-256 |
|---|---|
| `README.md` | `e5043b422a0a260f153cfed05b91640ddd8421ba5b76b559366abb572585b4cd` |
| `src/cli.rs` | `3d29159b7f982e48051045ae4542c62edd1e4333c1e85470d7a7593b11751811` |
| `src/cli/question.rs` | `e87900d9f37507691eddd0852bda14e00bfdb30b9dada7490501cc9c9f81a802` |
| `src/cli/help.rs` | `cab89b58f250712610c0f3160d4e4a5fe8aee16c481671c3916f550a699cbe26` |
| `src/cli/help/service.rs` | `e79bd8414a31697453fe8c1d93a36ea484a0b4c1b1b7fe6a6a5f401fdc7fb57d` |
| `src/control.rs` | `5972b218e2c57dcd4164875d5917314f962ac77930bf34a3b7a9cd4e71337ed2` |
| `src/control/unix.rs` | `eebaab23cbe858be00b66d01daafdff74deec772189589f59f2919eccb3b9d0b` |
| `src/store/records.rs` | `0bfb1279e027aaa68c6d995d1e72c594111a09669b9c1850de1257b2dd740927` |
| `src/supervisor.rs` | `59048bb9d6a89a3d6f5d8251e60253a9c87dc2ade5b03fd21c75d76bec2a4b0a` |
| `src/supervisor/manager.rs` | `3018b0d2b140ea7f8dd9b47f066d143d40bdde9332fc251086582f5771827fa4` |
| `src/codex/process.rs` | `78aa447b440f9d9a50bece124c2feda1c9a34290cdde709700a7fa169953387d` |
| `tests/local_service.rs` | `ff5b2c7ad63ac0bdf4e1bca77f6f95435dc78796fa02933ee1b66176285ada7f` |
| `tests/service_cancel.rs` | `18ca009f803e7d931ecf7af125c4f10a79499019e61530d6def14d3af2a66d17` |
| `12-HARNESS-COMMUNICATION.md` | `5f68086fe0accc82429517f993e4f4630722b7dd7fffa3b26dc94a640c691d21` |
| `ADR-0005-HARNESS-SERVICE-BOUNDARY.md` | `b767d226b52651c017e8ca9ff1324cd981e22b6f7fc6cb72e0c840c83f93b686` |
| `08-IMPLEMENTATION-CHECKPOINTS.md` | `25374b5016c3b33dde553202a95363de408c335ffaf22ffc8ef76fa204686ac4` |
| Installed release binary | `d02f0e393c8b33fecc441e54750c9c462f28dc933520c4bd48c760f464bd6e3c` |

## Design decisions

- Spewer owns durable task state and execution lifecycle.

- A harness adapter owns only host-run correlation, cursor storage, result persistence, resumption, and acknowledgement.

- The service protocol is engine-neutral. The Unix socket is a replaceable transport.

- Results are reads. Acknowledgements are explicit writes after parent persistence.

- Observation is nonblocking in version 0.1. The caller chooses its polling schedule.

- Cancellation becomes a durable terminal transition, not only a process signal.

- Capability negotiation precedes optional operation use.

- MCP, Play, Codex, and other harness integrations should project this operation set.

## Known limitations

- The service transport currently requires a Unix-domain socket.

- Observation is one-shot. Streaming notification transports remain future adapters.

- Codex App Server is the only production engine.

- MCP and a production Play continuation adapter are not part of CP13.

- Legacy `status`, `tail`, and `outbox` commands still support offline inspection. Harnesses should use the service-owned operations while it is running.

- Dollar cost requires a matching versioned price configuration.

## Documentation review

Desk route 4 covered the design, ADR, README, CLI help, and this evidence. The linter reported no failures.

- `WAIVE S-01`: Plain-text command tables and exact evidence rows are intentionally compact.

- `WAIVE S-04`: Protocol operation names, lifecycle terms, and Rust paths are fixed technical language.

- `WAIVE S-05`: Passive wording describes durable state where another actor would reduce clarity.

- `WAIVE T-02`: Ordered protocol steps remain independently scannable.

The skim order moves from purpose to protocol, verification, limitations, and the next boundary. Verdict: pass.

Next checkpoint: **build a thin Play adapter over CP13 without adding another task state machine**
