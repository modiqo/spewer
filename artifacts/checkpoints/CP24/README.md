# CP24 evidence: explicit unsandboxed Codex tasks

Status: **Complete**

## The user grants authority to one request

`spewer ask --danger-full-access` maps to Codex's `danger-full-access` sandbox and `dangerFullAccess` turn policy.

The alias `--no-sandbox` has the same effect. Both forms also grant network access to that request.

Spewer rejects the flag for Ollama capsules. Requests without the flag keep their existing sandbox policy.

Detached tasks store the exact permission in the durable request. Resume restores the same Codex sandbox.

`spewer capsule show` reports whether the selected capsule accepts the flag. It also prints a ready command example.

## Automated checks pass

The following commands passed on 2026-08-29:

```console
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-targets
./scripts/check-rust-source-lines.sh
./scripts/check-doc-lines.sh
./scripts/check-panic-primitives.sh
./scripts/check-codex-schema.sh
git diff --check
```

The test suite passed 75 tests. It covers parsing, the alias, protocol validation, Codex mapping, and Ollama rejection.

## Generic live checks pass

Ollama rejected an unsandboxed request before task acceptance:

```console
spewer ask "Reply with 20" --capsule qwen3-local --danger-full-access --detach
```

Spewer completed Codex task `tsk_ee787e907cf143c0b2228aa0` with the explicit flag. Its summary was `unsandboxed request accepted`.

The durable request stored `filesystem=danger-full-access` and `network=allow`. The task completed with no file changes.

## A stateful Play completed with explicit authority

Task `tsk_09d0be3f7752d6d08bd634a5` ran through the specialized `play-codex` capsule with
`gpt-5.6-luna` and `--danger-full-access`. The worker invoked the exact remote Play
`modiqo/retrieve-rideshare-receipts@0.1.8`.

The same task collected the start date, exclusive end date, and pull-and-run approval through typed
human input. After approval, Luna entered the Play-owned Gmail authentication step. The provider
browser opened, completed scoped OAuth, and returned control without sending credentials or tokens
through Spewer.

The verified Play result contained 23 rideshare receipts totaling $1,498.46. `spewer watch` retained
the capsule, model, safe Play command labels, input boundaries, and completion evidence. The Play
then yielded the optional recurrence choice without changing the completed domain result.
