# Play uses a durable inbox before it resumes

Status: **Accepted**

The Play adapter binds one owner-private Play run to one Spewer task. It stores the binding under `~/.rote-play/spewer` and never sends it to Spewer.

## The adapter has six retryable operations

| Operation | Durable transition | Safe retry result |
|---|---|---|
| `submit` | `prepared → submitted` | Same adapter job and Spewer task |
| `poll` | Advances the event cursor | Events after the stored cursor |
| `watch` | Repeats `poll` using Spewer's delay | One durable terminal inbox row |
| `claim` | `ready → claimed` | Same receipt for the same claim ID |
| `complete` | `claimed → applied → acknowledged` | Repeats acknowledgement |
| `pending` | None | Every job not yet acknowledged |

The adapter uses `play.spewer-adapter/v1` JSON outputs. Human text never replaces the structured receipt.

## The normal sequence preserves both owners

The harness follows this sequence:

```text
Play classifies bounded work
  → adapter submit
  → Spewer executes
  → adapter watch
  → adapter claim
  → harness resumes Play
  → adapter complete
```

Play owns classification, its continuation, and the final response. Spewer owns the task, scheduler, engine, checkpoints, and receipt.

## Submit persists intent before contacting Spewer

The adapter stores `host_run_id`, the private continuation reference, request hash, and request JSON first. It then calls Spewer with the request's stable idempotency key.

A lost submit response leaves a `prepared` job. Repeating `submit` asks Spewer again and receives the original task handle.

The adapter forces callback mode `poll` and consumer identity `play`. It rejects any request containing `private_continuation`.

## Poll stores the receipt before returning readiness

The adapter observes from its stored cursor. It uses Spewer's `poll_after_ms` value when `watch` schedules the next observation.

When Spewer becomes terminal, the adapter stores the complete outbox message in its inbox. The unique `receipt_id` and `message_id` reject changed redelivery.

## Claim separates receipt delivery from continuation state

The harness supplies a stable claim ID. Repeating the same claim returns the same receipt and resume token.

The command-line response never contains Play's continuation reference. A trusted in-process host can read that reference through `trusted_claim` because it already owns Play state.

A second claim ID fails closed. This prevents two harness turns from applying one receipt.

## Complete acknowledges only after a successful resume

The harness resumes Play with the claimed receipt. It calls `complete` only after the runtime has durably accepted that result.

The adapter commits `applied` before calling Spewer acknowledgement. A crash can therefore repeat acknowledgement without repeating the Play transition.

If the host cannot prove that Play accepted the receipt, it must not call `complete`. The durable claimed receipt remains available for reconciliation.

## The CLI teaches the state machine

The shortest shell path uses these commands:

```sh
play spewer submit \
  --host-run-id run_123 \
  --continuation-ref owner_private_ref \
  --request task.json

play spewer watch psj_example
play spewer claim psj_example --claim-id host_attempt_1
# Resume Play through the harness-owned continuation mechanism.
play spewer complete psj_example --claim-id host_attempt_1
```

`submit` returns the adapter job ID. `watch` returns the terminal projection and stored receipt metadata.

`claim` returns the immutable receipt and resume token. `complete` returns `status: acknowledged` after Spewer accepts the consumer-bound acknowledgement.

The CLI is an inspection and integration surface. A production Play host should call the adapter in process so the continuation reference never enters shell history or a process argument list.

## Model-visible tools remain smaller

Models should see `spewer_delegate`, `spewer_check`, and `spewer_cancel`. The harness runs cursor replay, inbox storage, claiming, and acknowledgement outside model context.

This follows Rippling's central interface lesson: agents perform better with a small capability surface and exact instructions. Spewer does not need an arbitrary code tool for eight lifecycle operations.

## Conformance tests cover adapter crashes

The adapter suite proves:

- a lost submit response resumes from durable prepared intent
- a duplicate submission returns one adapter job
- a duplicate receipt preserves identical bytes
- a repeated claim returns one resume token
- another claim ID cannot take the receipt
- public output omits the continuation reference
- a repeated completion safely repeats acknowledgement
- the database and its directory remain owner-private
- every command's help names its state transition and next action

Play is the first conformance adapter. Claude, Pi, Kimi, and other harnesses can implement the same inbox and claim contract without importing Play.
