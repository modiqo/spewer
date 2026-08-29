# Bounded web search gives local models current information

Status: **Accepted**

CP19 adds one optional tool to the Ollama adapter. It does not add a second interactive harness or
change the public task and receipt schemas.

Qwen inference remains local in both modes. `OLLAMA_API_KEY` authenticates the hosted search API;
it is not required for local inference.

## The task grants a narrow authority

The adapter exposes one model tool:

```text
web_search(query) -> up to five {title, url, content} results
```

The task must set `permissions.network` to `allow`. `spewer ask --web` makes that grant explicit.
Selecting Qwen as the default capsule does not grant network access.

The adapter rejects arbitrary URLs, browsers, commands, file writes, and unknown tools. It also
rejects malformed arguments and calls above the smaller of two limits:

- the task's `budgets.tool_calls` value;
- the adapter's fixed eight-call limit.

## The service owns the tool loop

Qwen selects a query through Ollama's structured tool-call format. Spewer validates the request,
calls Ollama's hosted search API, and returns the structured result as a tool message.

The loop continues until Qwen returns a final answer or crosses a limit. Spewer normalizes each
search start and completion into the existing event journal. The receipt counts every started tool.

## Runtime configuration controls advertising

The Spewer process reads `OLLAMA_API_KEY` from its environment. It advertises this card only when
the value is nonempty:

```json
{
  "network": true,
  "tools": ["web_search"]
}
```

Without configuration, the same capsule reports `network: false` and an empty tool list. A running
detached service cannot inherit a later environment change, so the user must restart it.

The adapter never copies the key into tasks, capsules, events, receipts, errors, or artifacts.
Redirects remain disabled. Search responses stop at 1 MiB, and each result snippet stops at 8 KiB.

## Acceptance proves behavior and containment

CP19 requires these results:

- A recorded Qwen response requests search, receives fixture results, and returns a final answer.
- Network-denied requests never enter the tool loop.
- Unknown tools, malformed arguments, excess calls, and missing credentials return typed errors.
- Capability lookup hides search without configuration and advertises it with configuration.
- Existing read-only Ollama questions still finish with zero tools.
- No credential marker appears in an event, receipt, error, log, or artifact.
- A live Qwen3 question returns current sourced information and records at least one tool call.

The live result closes CP19. Automated fixtures cannot prove that an external credential and hosted
search API work together.
