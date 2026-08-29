# ADR-0011: Plain ask uses a persisted capsule

Status: **Accepted**

Date: 2026-08-29

## Context

`spewer ask` previously used engine defaults unless the user supplied `--capsule`. Its attached
output also defaulted to JSON. A working local Qwen question therefore repeated selection and
format flags that did not express task intent.

Capsule capabilities and task permissions have different roles. A card advertises the maximum
available authority. The request still decides whether one task can use network access.

## Decision

Local configuration stores `default_capsule`. Existing version 1 files load `default` when the
field is absent. `spewer capsule default <id>` validates and persists an installed capsule.

Plain `spewer ask` binds the configured capsule. An explicit `--capsule` overrides it for one task.
Attached questions print answer text by default; `--json` requests the complete structured result.

`spewer capsule show [<id>]` reports the selected state, basic ask command, available `--web`
authority, output choices, and a detached-service capability check.

Network remains denied unless the user supplies `--web`. Selecting a capsule with `web_search`
does not silently expand a task's permissions.

## Consequences

The common local Qwen command becomes `spewer ask "<question>"`. Current-information questions add
only `--web`. Scripts that consumed attached JSON must add `--json`; detached output stays JSON.

The public task, receipt, capsule card, and control protocols do not change. Harnesses continue to
derive available tools from service capabilities and grant task authority explicitly.
