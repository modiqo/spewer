# External sources define upstream facts

Status: **Accepted**

Retrieved: 2026-08-29

## Official OpenAI documentation

- [Codex CLI installation](https://learn.chatgpt.com/docs/codex/cli) establishes the supported standalone installer and interactive sign-in flow used by `spewer install`.
- [Codex App Server](https://developers.openai.com/codex/app-server/) establishes the product purpose, open-source implementation, transports, JSON-RPC schema, lifecycle, generated schemas, events, approvals, model discovery, and recovery methods.
- [Codex SDK](https://developers.openai.com/codex/sdk/) establishes the supported programmatic path for starting, continuing, and resuming Codex threads.
- [Codex non-interactive mode](https://developers.openai.com/codex/noninteractive/) establishes the JSONL automation surface used by the reduced-capability adapter.

## Source-handling rules

Spewer documentation must link upstream behavior to an authoritative source. A design choice must use the label **Design choice** when readers could confuse it with upstream behavior.

Before CP0 accepts the design, verify the Codex repository license for any copied source or generated artifact. Open-source availability does not by itself settle reuse terms.

Before every compatibility release, regenerate schemas from the supported Codex binary. Record its version, schema hashes, and fixture hashes.

## Durability and agent-interface references

- [SQLite synchronous pragma](https://sqlite.org/pragma.html#pragma_synchronous) defines the durability guarantee for WAL mode with `synchronous=FULL`.
- [AWS transactional outbox guidance](https://docs.aws.amazon.com/prescriptive-guidance/latest/cloud-design-patterns/transactional-outbox.html) establishes atomic state-and-message commit, at-least-once delivery, and idempotent consumers.
- [Model Context Protocol tasks](https://modelcontextprotocol.io/specification/2025-11-25/basic/utilities/tasks) establishes durable task handles, polling, terminal states, and retention concepts. Spewer does not depend on experimental MCP task transport.
- [Rippling's MCP engineering post](https://www.rippling.com/blog/building-mcp-server) motivates a small model-visible tool surface, compressed action headers, latency-aware instructions, permission checks, and cross-harness evaluations.
