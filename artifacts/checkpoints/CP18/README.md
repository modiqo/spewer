# CP18 evidence: production local Qwen3 worker

Status: **Passed**

Starting revision: `a552e0cb74ccc67a2a7cc075cafbbb0005292c87`

Date: 2026-08-29

## Implemented behavior

- The production Ollama adapter discovers the live server version and installed local models.

- `spewer capsule add` registers a generic local model without replacing the default Luna capsule.

- Attached and detached `spewer ask --capsule` requests route through the selected capsule engine.

- One detached service schedules Codex App Server and Ollama tasks through the existing durable
  journal, recovery, cancellation, and receipt lifecycle.

- The local prompt contains the objective, acceptance criteria, notes, projected UTF-8 files, and
  the immutable accepted skill snapshot.

- The adapter rejects command allowlists, writable paths, and workspace-write authority. CP18 is
  read-only inference, not a local agent tool loop.

## Local installation evidence

| Component | Observed value |
|---|---|
| Ollama server | `0.33.1` |
| Local model | `qwen3:30b-a3b` |
| Ollama model size | 18 GB |
| Capsule | `qwen3-local` |
| Capsule engine | `ollama` |
| Current capsule state | `generic` |

`spewer doctor --engine ollama --model qwen3:30b-a3b` returned `ready: true`. It filtered the
pre-existing remote `kimi-k3:cloud` tag from the local model catalog.

## Live end-to-end evidence

The detached generic task `tsk_b16fcd8e87c532fb284c124a` completed through the service with
summary `DETACHED_QWEN_READY`.

| Receipt field | Observed value |
|---|---|
| Status | `completed` |
| Engine | `ollama` |
| Requested and observed model | `qwen3:30b-a3b` |
| Engine version | `ollama 0.33.1` |
| Input/output tokens | 109 / 234 |
| Tool calls | 0 |
| Wall time | 2,727 ms |
| Changed files | 0 |
| Final event sequence | 11 |

The service observed specialization without restarting. Binding the reference
`spewer-delegation` skill changed `qwen3-local` from generic to specialized. Attached task
`tsk_4cbaef6b3bae37bd7bfd58a9` returned `spewer-delegation` from the bound instructions.
Its receipt recorded skill revision `1d6201f39316`, the full skill digest, Qwen3, Ollama, and zero
tool calls. Unbinding restored the original generic capsule revision.

After the release binary replaced the development binary, service PID 51494 started with both
engine kinds. Attached task `tsk_1de8e772effc3c0eb99d4974` then returned
`RELEASE_QWEN_READY` through the installed binary and generic Qwen3 capsule.

## Automated results

| Command | Exit | Result |
|---|---:|---|
| `cargo fmt --all -- --check` | 0 | Formatting passed |
| `cargo clippy --all-targets --all-features -- -D warnings` | 0 | Strict linting passed |
| `cargo test --all-targets` | 0 | 60 tests passed |
| `RUSTDOCFLAGS="-D warnings" cargo doc --locked --no-deps` | 0 | Public documentation passed |
| `cargo deny check` | 0 | Policy passed with three existing unmatched-license warnings |
| `cargo machete` | 0 | No unused dependency found |
| Repository source, panic, document, and schema scripts | 0 | All local gates passed |
| `git diff --check` | 0 | Patch hygiene passed |

The unit fixture proves discovery, local-only model filtering, prompt injection, visible reasoning
removal, normalized events, and token usage. Existing service, cancellation, restart, task fixture,
fake-engine, and capsule tests prove compatibility with the unchanged lifecycle.

## Issues found and resolved

- Discovery initially reused its five-second timeout for inference. The adapter now uses five
  seconds for discovery and the task's wall budget for the model turn.

- Qwen3 returned a visible thinking block despite the non-thinking request flag. The prompt now
  includes `/no_think`, and the adapter removes any returned `<think>` block before journaling the
  answer.

- The original connected diagram hard-coded Luna. The CP18 visual names the selected capsule and
  shows Luna or Qwen3 without changing the frontier flow.

## Known limitations

- The Ollama endpoint is restricted to loopback HTTP for CP18.

- The adapter performs one finite inference request. It cannot execute commands, use tools, write
  files, resume a model run, or answer approval requests.

- Price remains unknown without a matching versioned price configuration.

- The fake engine remains deterministic test infrastructure. It does not run in the installed
  product or serve user tasks.

## Documentation review

Desk route 4 covered README, How It Works, the engine adapter contract, the checkpoint plan, and
this evidence packet. Its deterministic linter reported no failures.

The installation path stays progressive: install Spewer, optionally pull Qwen3, verify Ollama, add
the capsule, and ask through it. Verdict: pass.
