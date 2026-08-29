# Installation and capsules make worker state discoverable

Status: **Accepted**

CP15 adds the smallest setup surface that produces a useful Spewer service. It does not add a second connection ceremony between Spewer and a frontier harness.

## `spewer install` owns first-run readiness

The command follows one ordered path:

1. Find a working Codex CLI.
2. If it is absent, run the official Codex installer unless `--skip-codex-install` was supplied.
3. Create private Spewer defaults when none exist.
4. Ensure the `default` capsule exists with `gpt-5.6-luna` and no skill binding.
5. Verify the Codex App Server handshake.
6. Start or reuse the detached Spewer service.

Codex authentication remains an interactive Codex responsibility. Spewer reports the exact next action when the installed CLI cannot authenticate; it does not store or proxy credentials.

Repeating installation preserves an existing configuration and reuses a ready service. `spewer init --overwrite` remains the explicit way to replace configuration.

## A capsule describes a dispatchable worker

The first installation has one capsule named `default`. Its engine is Codex App Server and its model is `gpt-5.6-luna`.

A capsule without a skill binding advertises `generic`. Binding a valid `SKILL.md` changes the same capsule to `specialized`. Unbinding the skill restores `generic`; it does not delete the capsule or engine configuration.

The persisted binding contains the skill name, description, content digest, revision, and canonical source file. Capability responses omit the local source path.

## Capability lookup is live

The running service reads the capsule catalog for every capability request. An adapter can therefore discover a new binding without reconnecting or regenerating code.

The capability revision is a SHA-256 digest of the sorted public capsule advertisements. Equal catalogs have equal revisions across restarts. Any advertised change produces a different revision.

The stable control protocol and the live catalog have separate lifecycles:

- regenerate an adapter only when its stable protocol contract changes;
- refresh capability lookup when the content revision changes or its cache expires.

## The commands stay small

```text
spewer install [--workspace <path>] [--max-workers <count>]
spewer capsule list
spewer capsule bind <capsule-id> <skill-or-directory>
spewer capsule unbind <capsule-id>
```

`capsule list` emits the same public catalog shape that appears in service capabilities. Bind and unbind are explicit local administration operations; model-facing plugins only need discovery, delegation, checking, and cancellation.

## Security and failure behavior

Capsule directories and files are owner-private on Unix. Writes use a new temporary file, flush it, rename it into place, and flush the containing directory.

A binding is rejected when the file is missing, too large, lacks simple `name` and `description` front matter, or names a capsule that does not exist. A failed write leaves the previous manifest intact.
