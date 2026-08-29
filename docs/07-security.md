# Spewer grants each task the minimum declared authority

Status: **Accepted**

Spewer controls the worker's environment, tools, and approval path. An engine sandbox complements these controls but does not replace them.

## Every task declares authority before dispatch

The task request names filesystem, network, command, secret, and external-service permissions. The controller rejects unsupported combinations.

Version 0.1 defaults to:

- one isolated worktree
- workspace-write filesystem access
- denied network access
- no inherited secrets except an explicit allowlist
- command policy enforced by the engine and Spewer
- parent approval for effects outside the worktree

## The workspace bounds file changes

Spewer resolves and validates the workspace path before starting a worker. It rejects broad roots, unresolved links, and paths outside configured repositories.

The worktree manager records the base revision and allowed path patterns. A verification step rejects changes outside those patterns.

Spewer retains the final diff as an immutable artifact. The parent decides whether to apply, merge, or discard it.

## Process launch avoids ambient authority

Spewer passes an explicit environment to Codex App Server. It does not forward the complete parent environment.

Temporary directories use dedicated paths with restrictive permissions. Process identifiers and transport endpoints remain task-scoped.

Stdio is the default transport. WebSocket transports require local binding or authenticated encryption.

## Approvals cross a durable boundary

Spewer records an approval request before showing it to the parent. It records the parent's answer before forwarding it to the engine.

Approval records include the requested action, scope, requester, responder, answer, timestamp, and source event hash. A stale approval cannot authorize a different action.

## External effects require idempotency

Every external write uses a stable effect key derived from the task and operation. The effect table records `planned`, `started`, `verified`, or `uncertain`.

Recovery retries only a planned effect that never started. An uncertain effect requires inspection or parent escalation.

## Secrets remain references

Task requests reference named credentials without embedding values. A credential broker resolves the smallest required secret at dispatch time.

Logs, events, receipts, and artifacts run through redaction before persistence. Hashes help correlate repeated values without storing them directly.

## Engine capabilities cannot expand task permissions

The Codex adapter translates Spewer permissions into supported App Server settings. A missing engine control causes rejection or a stronger outer isolation boundary.

Spewer records the requested and effective permissions. A weaker effective policy cannot proceed silently.

## Security gates block release

CP6 requires path-escape tests, secret-redaction tests, approval replay tests, and side-effect recovery tests. Any silent authority expansion blocks the checkpoint.
