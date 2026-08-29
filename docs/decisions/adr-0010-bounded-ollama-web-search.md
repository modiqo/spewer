# ADR-0010: keep bounded web search inside the Ollama adapter

Status: **Accepted**

## Context

The CP18 Ollama adapter performs one inference request and advertises no tools. A local model cannot
answer current-information questions without an external result.

A complete interactive harness would duplicate run state, permissions, tools, and configuration.
Spewer already owns the durable task lifecycle and needs one smaller capability.

Ollama provides hosted `web_search` and `web_fetch` APIs plus a Qwen3 tool-loop example. Search
requires a free Ollama account and an API key.

## Decision

CP19 adds only `web_search(query)` to the Ollama adapter. Spewer validates the model request,
executes the search, returns structured results, and journals observable tool events.

The task must authorize network access. The adapter reads `OLLAMA_API_KEY` from its process
environment and never copies the value into a task, capsule, event, receipt, error, or artifact.

An Ollama capsule advertises web search only when the current process has a nonempty key. The live
catalog revision changes when this runtime capability changes.

The adapter rejects unknown tools and bounds search results, tool calls, response bytes, and wall
time. CP19 does not add `web_fetch`, arbitrary HTTP, a browser, commands, or writes.

## Consequences

Local Qwen inference remains on the user's machine, while search queries and results cross the
Ollama hosted-search boundary.

Attached commands observe credentials from their shell. A detached service must start after the
credential is available because a running process cannot inherit a later environment change.

The public task and receipt schemas remain engine-neutral. A later provider or MCP server can
implement the same normalized tool without changing the parent harness actions.
