# ADR-0004: Spewer uses minimal Rust with a bounded Tokio shell

Status: **Accepted**

Date: 2026-08-28

## Context

Spewer must supervise long-running processes, stream JSON-RPC, enforce timeouts, react to signals, and deliver durable results. Its recovery semantics must remain deterministic and independent of live tasks or futures.

The project also needs a small dependency surface, quick incremental builds, explicit errors, and strict source-size and panic-safety gates.

## Decision

Version 0.1 is one Rust package with library and binary targets. Handwritten Rust files remain at or below 500 physical lines.

Tokio runs only the outer driver: process I/O, timers, signals, bounded channels, and child lifecycle. The reducer, protocol validation, budget evaluation, and SQLite transactions remain synchronous and deterministic.

Tokio uses a current-thread runtime and an explicit feature list. Spewer does not use Tokio's `full` feature, `async-trait`, detached tasks, unbounded channels, or database work on runtime threads.

## Consequences

The runtime can multiplex App Server events and cancellation without embedding async state in durable records. A dedicated database thread owns SQLite, and async callers use bounded commands and one-shot replies.

The first package stays cohesive. A workspace or custom compiler-extension crate requires a demonstrated second ownership boundary.

## Rejected alternatives

Bun would reduce initial JSON-RPC code but weaken compile-time enforcement around state transitions, process ownership, and durable interfaces.

Using only blocking threads would work, but Tokio already supplies the exact cancellation, timeout, signal, and process primitives required at the outer boundary.

## Review trigger

Review this decision when profiling shows the current-thread runtime is saturated or a second engine creates a stable package boundary.
