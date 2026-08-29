# CP19 evidence: bounded web search for local models

Status: **Complete**

Starting revision: `ab3d597592125bc3affcbcff3078863cc0f81b05`

Date: 2026-08-29

## Implemented behavior

- An Ollama capsule advertises `web_search` only when its process can read a nonempty
  `OLLAMA_API_KEY`.
- `spewer ask --web` grants network access explicitly and rejects capsules without that tool.
- Detached ask reads the service's card, preventing a newer shell environment from overstating an
  older service's authority.
- Qwen can request `web_search(query)`, receive up to five structured results, and produce a final
  answer through the existing journal and receipt lifecycle.
- The adapter rejects unknown tools, malformed arguments, and calls above the smaller of the task
  budget and the eight-call adapter limit.
- Search redirects are disabled, responses stop at 1 MiB, and the key never enters durable state.

## Automated evidence

The recorded adapter test executes this complete sequence:

```text
question -> Qwen tool request -> authenticated fixture search -> tool result -> Qwen answer
```

It observes one normalized tool call, aggregates usage across both model turns, and confirms that
the fixture key does not appear in engine events.

`cargo test --all-targets` passed 68 tests after the CP19 changes. The focused library set includes
44 tests.

The remaining automated gates also pass: formatting, Clippy with warnings denied, rustdoc with
warnings denied, `cargo deny`, `cargo machete`, source and documentation line limits, the panic
primitive audit, the Codex schema manifest check, documentation lint, and `git diff --check`.
`cargo deny` reports only the known duplicate `windows-sys` versions and unused `Zlib` allowance.
The CA root data used by HTTPS is explicitly allowed under `CDLA-Permissive-2.0`.

## The user confirmed the live gate

The user restarted Spewer from the credential-owning terminal and ran this command:

```console
$ spewer ask "What is the current weather in Sunnyvale, California?" \
    --capsule qwen3-local --web --text
```

No live secret or search response appears in this evidence packet.
The user reported that the sourced answer worked as expected. This attestation closes the external
credential gate; the automated fixture retains the reproducible protocol evidence.
