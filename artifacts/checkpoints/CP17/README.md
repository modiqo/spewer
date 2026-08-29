# CP17 evidence: frontier integration kit

Status: **Passed**

Starting revision: `cf71cd68e57509055ada17000569dc4e0fc34fba`

Date: 2026-08-29

## Implemented behavior

- The public harness client provides discovery, live capsule-bound delegation, combined checking, and cancellation.

- `spewer delegate`, `spewer check`, and the existing `spewer cancel` form the three-action frontier surface.

- Delegation replaces caller capsule and engine fields with the selected live advertisement. The service independently validates that revision before acceptance.

- Check returns projection, later events, next cursor, polling delay, terminal result, and an explicit readiness flag.

- The bundled `spewer-delegation` Agent Skill keeps classification, user communication, receipt judgment, and the final answer in the frontier harness.

- `spewer install` installs the reference Codex skill idempotently and refuses to overwrite different user content.

## Automated results

| Command | Exit | Result |
|---|---:|---|
| Agent Skill quick validation | 0 | Bundled skill passed Skill Creator validation |
| `cargo fmt --all -- --check` | 0 | Formatting passed |
| `cargo clippy --all-targets --all-features -- -D warnings` | 0 | Strict linting passed |
| `cargo test --all-targets` | 0 | 53 tests passed |
| `RUSTDOCFLAGS="-D warnings" cargo doc --locked --no-deps` | 0 | Public documentation passed |
| `cargo deny check` | 0 | Policy passed with three existing unmatched-license warnings |
| `cargo machete` | 0 | No unused dependency found |
| Repository source, panic, document, and schema scripts | 0 | All local gates passed |
| `git diff --check` | 0 | Patch hygiene passed |

The installed-binary test proves installation and reuse of the skill, missing-capsule rejection, live specialized discovery, delegated execution, combined checking, exact skill prompt injection, and matching receipt evidence without restarting Spewer.

## Issue found and resolved

The first combined-check response required callers to infer readiness from a nested optional message. CP17 now returns an explicit `ready` field while retaining the full stable result.

## Artifact hashes

| Artifact | SHA-256 |
|---|---|
| `docs/17-frontier-integration.md` | `31d7a4c6194d8977f118bca1a16c8a95f05042a1cb08f62c3f9567262eb8c396` |
| `docs/decisions/adr-0009-three-action-frontier-surface.md` | `594e21f5aa78e484eb93537a73e2e8dfa4192cc0e9f21b8564e016abc9b51185` |
| `src/harness.rs` | `b683e7cfb794731e30bec2f3c2fe57996c39cf115dc8968ad5e39d20dfd40e8a` |
| `integrations/codex/spewer-delegation/SKILL.md` | `1d6201f393166d91a94d1172f0cff955d6fab2fedbb265f7246305153fbf08ed` |
| `tests/install_capsules.rs` | `f6d480ad1a9d2aa87c87184fbe47c2b50be5839fadd5d4ae81ce6b7fd688d50f` |
| `docs/how_it_works.md` | `2b37a5b3f660f67a506e8501c695e9eeb9ffe38196684a12dd01b0334acebc40` |

## Known limitations

- The generic client cannot persist a foreign harness continuation. Play remains the complete durable parent adapter; other hosts must close that boundary themselves.

- The Codex integration is a reference Agent Skill over the CLI, not a native plugin with custom UI.

- The skill does not acknowledge receipts automatically because it cannot prove that the host durably applied them.

- This proof uses a protocol-compatible fake App Server. The user will run the final frontier-to-worker model test after handoff.

## Documentation review

Desk route 4 covered the integration guide, ADR, skill instructions, README, How It Works update, and this packet. Its deterministic linter reported no failures.

The skim order moves from behavior to proof, the resolved issue, artifact identity, limitations, and verdict. Verdict: pass.
