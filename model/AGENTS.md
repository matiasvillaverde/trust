# Model crate guide

This file extends the repository `AGENTS.md` for `model/`.

## Responsibility

`trust-model` owns domain entities and cross-crate contracts. It must not depend on core, persistence, CLI, or a concrete broker. Keep types broker-neutral and persistence-neutral.

- `broker.rs`: `Broker`, typed broker errors, logs, and broker-facing result types.
- `database.rs`: `DatabaseFactory` and narrow read/write traits.
- `trade.rs` and `order.rs`: lifecycle state and the three-order trade aggregate.
- `account.rs`, `level.rs`, and `rule.rs`: shared invariants that callers should not duplicate.

## Compatibility and invariants

- A public type or trait change is workspace-wide. Update `db-sqlite`, core mocks/test factories, both broker adapters, the CLI, and tests as applicable.
- Prefer adding a default implementation to optional broker capabilities when backward compatibility is appropriate; required lifecycle operations must remain explicit.
- `DatabaseFactory` deliberately returns specialized trait objects. Add a capability to the narrowest read/write trait and wire its factory accessor rather than creating a generic database escape hatch.
- Keep `Trade.balance` semantics clear: it is a persisted projection and may be stale while a trade is open. Mutation code must reload authoritative state.
- Preserve account hierarchy validation, level bounds/multipliers, status parsing/display values, and broker-kind identity as public contracts.
- Use `Decimal` and checked arithmetic for finance. Legacy risk-rule percentage payloads use `f32`; do not spread that representation into new financial calculations.
- Production code is subject to the strict crate-level Clippy denies. Return typed errors instead of panicking.

## Tests

Place pure entity/parse/invariant tests beside the module. For a contract change, also run affected implementation crates.

```bash
cargo test -p trust-model
cargo clippy -p trust-model --all-targets --all-features -- -D warnings
```
