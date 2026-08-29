# CP15 evidence: one-command installation and live capsules

Status: **Passed**

Starting revision: `9e85561c43d18fb02f3015273cfa30409c1f52f5`

Date: 2026-08-29

## Implemented behavior

- `spewer install` finds a working Codex CLI or runs the official installer.

- Installation preserves an existing configuration, initializes a missing one, ensures the default Luna capsule, verifies App Server, and starts or reuses the detached service.

- The default capsule persists in an owner-private, atomically replaced JSON manifest.

- `spewer capsule list`, `bind`, and `unbind` expose one explicit administration surface.

- Binding validates a bounded UTF-8 `SKILL.md` and records its name, description, revision, digest, and canonical local source.

- Service capabilities advertise only safe capsule and skill metadata. They omit the local skill source.

- Capability lookup reads the current catalog for every request. Its content hash changes on bind and returns to the prior value on unbind without a service restart.

## Automated results

| Command | Exit | Result |
|---|---:|---|
| `cargo fmt --all -- --check` | 0 | Formatting passed |
| `cargo clippy --all-targets --all-features -- -D warnings` | 0 | Strict linting passed |
| `cargo test --all-targets` | 0 | 52 tests passed |
| `RUSTDOCFLAGS="-D warnings" cargo doc --locked --no-deps` | 0 | Public documentation passed |
| `cargo deny check` | 0 | Advisory, license, ban, and source policy passed with three existing unmatched-license warnings |
| `cargo machete` | 0 | No unused dependency found |
| Repository source, panic, document, and schema scripts | 0 | All local gates passed |
| `git diff --check` | 0 | Patch hygiene passed |

The new end-to-end test runs the installed binary against a protocol-compatible App Server. It proves first installation, duplicate installation, private defaults, detached startup reuse, generic discovery, specialization, safe advertisement, unbinding, and live capability revision changes.

## Live installed result

An isolated temporary Spewer home ran `spewer install --skip-codex-install` against Codex CLI `0.151.0`.

The command completed the real App Server handshake, advertised one generic `default` capsule, started the detached service, and returned ready. A separate capability lookup returned revision `7c3da4acdbe573da3cf4ba125714082a3309e6dfb28d85c163e839ecb32edfca` and the same generic capsule.

The service stopped cleanly. The temporary Spewer home was moved to the system Trash.

No model turn ran during this proof. The existing live Luna execution evidence from CP14 remains the model-execution proof.

## Artifact hashes

| Artifact | SHA-256 |
|---|---|
| `README.md` | `bc67454316933038e4ccfa833b3dd96b6012cacb9fb5766e4b1e98107468dad1` |
| `docs/15-install-and-capsules.md` | `dbc548aeca5261fd20da03cb9ed0f246920c8d56b506a4f37a9c99168f7a3802` |
| `docs/decisions/adr-0007-live-capsule-catalog.md` | `ff8378b55775983d26843cfa20c4c72273e5230fb4566329bf970b462daa763c` |
| `src/capsule.rs` | `240769166e3247fff89c737a5bccef7727a7cf0b7cb3e311a65d196cece662da` |
| `src/cli/setup.rs` | `6e25473d2266262a1b349ff42134e799a053d58bdba61f49c3a99ec2cf2e98c9` |
| `src/control.rs` | `09a69734829cec8807bf140a6421f5051713ff96abd33c400bb428b77be1c29e` |
| `tests/install_capsules.rs` | `5e547fe05cfc0d17540c38c8585e3cc299f779c897c5dcf4f009606ab8b79cab` |

## Known limitations

- Skill discovery accepts simple single-line `name`, `description`, and optional `version` front matter. CP16 owns a versioned skill input contract.

- Tasks do not select a capsule yet. CP16 binds acceptance and receipts to the advertised capsule revision.

- Codex authentication remains interactive through `codex`; Spewer does not store credentials.

- The installer supports the current macOS and Linux path. A second production worker and frontier plugin remain CP18 and CP17 work.

## Documentation review

Desk route 4 covered the setup reference, ADR, checkpoint plan, How It Works status, README tutorial, sources, and this packet. Its deterministic linter reported no failures.

The remaining noun-cluster and line-length warnings describe exact commands, protocol fields, evidence lists, and established technical names. They remain precise under Desk Law 1.

The skim order moves from implemented behavior to automated proof, live proof, artifact identity, limitations, and verdict. Verdict: pass.
