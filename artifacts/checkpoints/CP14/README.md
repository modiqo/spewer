# CP14 evidence: crash closure and Play adapter

Status: **Passed**

Starting revision: `a313ac90e9ff3502249f126ea36c13f3dad3f99d`

Date: 2026-08-29

## Implemented behavior

- Task acceptance commits the request, first event, attempt, and dispatch intent in one SQLite transaction.

- An idempotency key binds the canonical request. A changed request cannot reuse it.

- A dispatch lease records the server epoch, worker, deadline, process group, executable signature, and process start identity.

- Spewer records App Server process custody before sending `initialize`.

- Startup rebuilds pristine queued work before reporting readiness.

- Startup escalates work with execution evidence instead of replaying an uncertain effect.

- Startup verifies and reaps a matching orphan process group. It refuses a mismatched process identity.

- Every task declares a callback consumer. Pending delivery and acknowledgement enforce that identity.

- Observations include `poll_after_ms` so adapters use service-directed scheduling.

- Play stores prepared submissions, task handles, cursors, terminal messages, claims, and application state in an owner-private SQLite inbox.

- Play persists before submit, receipt readiness, and acknowledgement boundaries. Retries preserve the same job, task, receipt, and claim identity.

- Public Play adapter output never contains the continuation reference.

## Automated results

| Command | Exit | Result |
|---|---:|---|
| `cargo fmt --all -- --check` | 0 | Formatting passed |
| `cargo clippy --all-targets --all-features -- -D warnings` | 0 | Strict linting passed |
| `cargo test --all-targets` | 0 | 48 Spewer tests passed |
| `RUSTDOCFLAGS="-D warnings" cargo doc --locked --no-deps` | 0 | Public documentation passed |
| `cargo deny check` | 0 | Advisory, license, ban, and source policy passed |
| `cargo machete` | 0 | No unused dependency found |
| Repository source, panic, document, and schema scripts | 0 | All local gates passed |
| `just check` in Play | 0 | Packaging, layout, machine, typing, prompt, and typo gates passed |
| `uv run python -m unittest discover` in Play | 0 | 679 Play tests passed |
| `git diff --check` in both repositories | 0 | Patch hygiene passed |

The restart test launches a protocol-compatible App Server process, commits its custody, kills Spewer with `SIGKILL`, proves the App Server survived, and restarts Spewer. Recovery verifies the process signature, reaps its process group, and produces one durable escalation receipt.

The dispatch tests also cover pristine lease replay and conservative escalation after observable worker start. Play tests cover lost responses, duplicate submission, private permissions, immutable redelivery, stable claims, continuation privacy, and state-directed command help.

## Live installed results

The release binary was installed at `/Users/chetanconikee/.cargo/bin/spewer`. A detached service reported ready with one worker and the complete version 0.1 operation set.

An attached smoke test asked Luna to multiply 17 by 19. Task `tsk_09a4f56b0b26092e7df5207f` returned `323` with `gpt-5.6-luna`, zero tools, and 4,086 milliseconds of wall time.

The Play adapter then submitted a separate task through the live Spewer socket. Adapter job `psj_18cbeff309945d64f117c74a` mapped to task `tsk_254d49c67836577c007c69e4`.

Codex App Server observed `gpt-5.6-luna` and returned `899` for 29 multiplied by 31. The receipt recorded 16,949 input tokens, 9,984 cached input tokens, 145 output tokens, 138 reasoning tokens, zero tool calls, and 5,492 milliseconds.

Play stored message `msg_45508fb303258b2357a222a9`, claimed the receipt without printing its continuation reference, committed host application, and acknowledged it as consumer `play`. Both the Play pending list and Spewer's `play` outbox were empty afterward.

Provider cost remained unknown because no matching price row was configured. The service remains ready for local use.

## Artifact hashes

| Artifact | SHA-256 |
|---|---|
| `README.md` | `1ff2dcdb66f355cc40a77f0d962d72f19700878312f08f91a8548d6cfcac82ce` |
| `src/protocol.rs` | `1a826fd405451bf0f1a5704fcdf04d5b1d420340c052a54773dec3771f951daf` |
| `src/store/dispatch.rs` | `906b43347098f39ce019a7bb6fa68d0cf3cc74c03926fa821b021119b6ee91d7` |
| `src/store/records.rs` | `13922a58b4b558c91e26aac1002223db67afe9957d36c0c057baf198c52fc632` |
| `src/store/schema.rs` | `754b874be666d97f36920424328d22a5c2fec77241e18bcbd017161fe9f70eaa` |
| `src/supervisor/manager.rs` | `a1b578434b55f913958b3d5b9a20a269833f080628cabec18eefd38f85dd196a` |
| `src/supervisor/process_custody.rs` | `5d10b1c2fbdb77660445690af04cc124f84fdfa0023e7d006b4fa63285bf632b` |
| `tests/durable_dispatch.rs` | `42c5806724c1373d70da9bf10ab7c6c5fe9f01988cfd9f358632fe321983776c` |
| `tests/service_restart.rs` | `0f82dc9ece33fc25035f2a82b10974da8c9ad14d19716330fd8a0ccf3d3d843e` |
| Play `scripts/lib/play/spewer.py` | `54b4885e5ed730e3912a34f7cf7d875c4ce9dbcd94d82eee8e019ab65c8829bd` |
| Play `scripts/bin/play-spewer` | `e67fc70662864d8a85fe29cb0f5b86054bd44a4930d36e5ffb15c5d0d5a0e731` |
| Play `tests/foundation/test_spewer.py` | `e3739aba2895cec397805d9f3d4fb090ebb5c515033b5ff8098b4b586755210b` |
| Installed release binary | `96fefc90d4bdf62e03b30f8cd5a8c5d4fe1912cd80a02710f1aa4ac703d63c79` |

## Durable boundary verdict

CP14 closes both requested crash windows.

Spewer can recover accepted work and App Server custody without silent loss or blind replay. Play can recover receipt delivery without losing its continuation or acknowledging before application.

The protocol is ready for another harness adapter. That adapter still needs its own conformance evidence; Spewer does not make a foreign harness durable by itself.

## Documentation review

Desk route 4 covered the crash-closure explanation, Play adapter reference, ADR, checkpoint, READMEs, and this evidence packet. The linter reported no failures.

- `WAIVE S-01`: Lifecycle tables and evidence rows remain compact because exact comparison is their purpose.

- `WAIVE S-04`: Protocol identities, Rust paths, and durable-state terms are fixed technical language.

- `WAIVE T-02`: Ordered lifecycle steps remain independently scannable.

The skim order moves from guarantees to tests, live proof, artifact identity, and verdict. Verdict: pass.
