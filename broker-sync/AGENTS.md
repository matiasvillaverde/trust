# Broker sync guide

This file extends the repository `AGENTS.md` for `broker-sync/`.

## Current scope

This crate currently contains an in-memory std-thread session actor, serialized command/event contracts, and a separate broker connection state machine. It is scaffolding: `StartSync`, `StopSync`, and `ManualReconcile` compatibility commands do not perform websocket synchronization, and the state machine is not wired into the actor.

## Contracts and invariants

- `messages.rs` enums are serde-tagged public contracts. Variant/field changes and duration encoding changes are breaking unless deliberately migrated.
- Redact credentials from connection events with the existing constructors; never expose URL query strings or passwords.
- Sessions are keyed only by trade ID. Starting twice replaces the entry, list order is nondeterministic, touching a missing session is ignored, and stopping a missing session still emits an event. Do not write tests that assume otherwise unless intentionally changing the contract.
- Event send failures are currently ignored and the actor has no join handle. Account for handle drop/shutdown semantics when changing lifecycle behavior.
- Preserve the state path `Disconnected -> Connecting -> Reconciling -> Live`, explicit invalid transitions, saturating/capped exponential backoff, and bounded jitter. `Disconnect` is currently not implemented by the transition table.
- Avoid timing-flaky tests. Use `transition_at`, zero jitter, or bounded assertions. Keep the Proptest regression file committed.

```bash
cargo test -p broker-sync
```

If wiring real broker I/O, define acknowledgement, retry/idempotency, shutdown, ordering, and reconciliation semantics explicitly before replacing the current no-ops.
