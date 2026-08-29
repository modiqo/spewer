# CP16 evidence: capsule-bound execution

Status: **Passed**

Starting revision: `cf71cd68e57509055ada17000569dc4e0fc34fba`

Date: 2026-08-29

## Implemented behavior

- Task requests can select an advertised capsule ID and content revision.

- Spewer validates the current capsule, engine, model, and skill digest before it commits task acceptance.

- Accepted specialized work stores the exact bounded `SKILL.md` instructions. Later edits, binds, and unbinds cannot change queued or recovered execution.

- Worker prompts receive the accepted instruction snapshot. Capability responses and receipts omit its text and local source path.

- Terminal receipts identify the capsule revision, generic or specialized kind, and safe skill evidence.

- Existing version 0.1 requests remain valid when they omit a capsule.

## Automated results

| Command | Exit | Result |
|---|---:|---|
| `cargo fmt --all -- --check` | 0 | Formatting passed |
| `cargo clippy --all-targets --all-features -- -D warnings` | 0 | Strict linting passed |
| `cargo test --all-targets` | 0 | 53 tests passed |
| `RUSTDOCFLAGS="-D warnings" cargo doc --locked --no-deps` | 0 | Public documentation passed |
| `cargo deny check` | 0 | Policy passed with three existing unmatched-license warnings |
| `cargo machete` | 0 | No unused dependency found |
| Repository source, panic, document, and schema scripts | 0 | All local gates passed |
| `git diff --check` | 0 | Patch hygiene passed |

Unit tests prove stale-selection rejection, edit detection, immutable accepted snapshots, and recovery from stored instructions. The installed-binary test proves that a specialized task receives the exact bound instructions and returns matching receipt evidence.

## Issue found and resolved

The first direct execution path could trust a caller-supplied binding snapshot. New work now always discards that private field and resolves the local manifest. Only already-accepted recovery work may reuse its stored snapshot.

## Artifact hashes

| Artifact | SHA-256 |
|---|---|
| `docs/16-capsule-bound-execution.md` | `0752f96d9d3dbbe663ce38709c8dd3ab14e037a61324f4752c94304adc596525` |
| `docs/decisions/adr-0008-snapshot-capsule-before-acceptance.md` | `d4298959f2ef6821d330be5472e3bec86f98bd7b9df52c212fb021859e831e45` |
| `src/capsule/binding.rs` | `c7abed06171403bb3a6064f2f46f7fb3ff94e2966f0b305ab8407f402b5fbe39` |
| `src/capsule/selection.rs` | `1e325169aef45c1fe16885f7d0156a1e2e6841c46705e6301ebed9c84e3ae10c` |
| `tests/install_capsules.rs` | `f6d480ad1a9d2aa87c87184fbe47c2b50be5839fadd5d4ae81ce6b7fd688d50f` |

## Known limitations

- Capsule matching remains a frontier decision. Spewer validates a requested live capsule but does not rank candidates.

- Accepted request storage contains specialized instructions and therefore remains owner-private.

- This proof uses a protocol-compatible fake App Server for exact prompt inspection. CP14 retains the live Luna model-turn evidence.

## Documentation review

Desk route 4 covered the protocol reference, ADR, implementation checkpoint, How It Works update, and this packet. Its deterministic linter reported no failures.

The skim order moves from behavior to proof, the resolved issue, artifact identity, limitations, and verdict. Verdict: pass.
