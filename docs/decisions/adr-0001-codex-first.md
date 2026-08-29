# ADR-0001: Codex App Server is the first Spewer engine

Status: **Accepted**

Date: 2026-08-28

## Context

Spewer needs a worker harness with streamed progress, tools, approvals, model selection, and resumable history. Building those features before testing the supervisor would mix two large problems.

Codex App Server already exposes these primitives through a documented protocol. OpenAI also publishes its implementation as open source.

## Decision

Version 0.1 uses Codex App Server as its first complete engine adapter. Spewer starts it locally and consumes its JSON-RPC stream.

The adapter uses generated schemas for the installed Codex version. It maps Codex notifications into Spewer events before they reach core packages.

## Consequences

The first release can focus on supervision, durability, budgets, receipts, and cost measurement. It inherits upstream protocol changes and must test supported Codex versions.

This decision does not make Codex protocol Spewer's public protocol. ADR-0002 preserves that boundary.

## Rejected alternatives

Building a new worker harness first delays validation of Spewer's main idea. Using only `codex exec --json` weakens interactive control and recovery.

Embedding provider APIs directly would force Spewer to build its own tool loop immediately. That remains a later engine-server project.

## Review trigger

Review this decision after CP8 or when App Server cannot express a required permission, recovery, or telemetry behavior.
