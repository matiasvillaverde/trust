# Core crate guide

This file extends the repository `AGENTS.md` for `core/`.

## Responsibility

Core owns risk policy, calculations, and application orchestration. CLI business operations should enter through `TrustFacade`; public calculators and services remain available for focused composition and tests.

- `lib.rs`: facade construction, broker registry use, caches, protected mode, reports, and high-level workflows.
- `commands/`: lifecycle writes and accounting coordination.
- `validators/`: preconditions and risk guards.
- `calculators_*`: pure financial projections and metrics.
- `services/`: leveling, distribution, grading, tax, wash-sale, advisory, and transfers.
- `events/`: post-trade event data/handling.

## Lifecycle rules

- Reload a trade by ID before mutation; do not trust caller-supplied status or cached balance.
- Funding validates persisted state, rules, available capital, and level-adjusted sizing before status, transaction, and balance changes.
- Validate before broker I/O. Route brokers by the account's `BrokerKind` through `BrokerRegistry` and preserve typed unsupported-asset errors.
- A filled long stop may not move down; a filled short stop may not move up. Stop changes must not widen risk.
- Preserve transaction/balance/order/status auditability. Use named savepoints where a multi-write workflow must roll back as a unit; broker sync already uses this path. Test both success and mid-operation failure.
- Closing a trade evaluates leveling and conditionally distributes positive profit. Grading is explicit. Preserve the distinction and existing error-propagation semantics when modifying close paths.
- Facade distribution and level snapshots are caches. Configuration or history changes must update/invalidate them consistently.
- Protected authorization is one-shot keyword authorization. Argon2 helpers hash/verify stored passwords separately; do not conflate the two mechanisms.

## Tests

Runnable coverage is in inline module tests, `src/security_tests.rs`, and `tests/`. The historical unregistered `src/integration_tests.rs` simulation has been removed. Cash contracts now run through the public facade and real SQLite in `tests/capital_contracts.rs`; lifecycle and broker scenarios are in `tests/reliability.rs` and `tests/security_regressions.rs`.

```bash
cargo test -p core
cargo test -p core --test risk_invariants_test -- --test-threads=1
make trust-proof
```

Use mocks in `src/mocks.rs` for external boundaries and focused unit tests. For money-moving workflows prefer the public facade with real, isolated SQLite, independent expected amounts, persisted balances/receipts, and failure/retry assertions. Add property tests for general financial invariants and regression tests for every lifecycle failure mode changed. Follow [the testing guide](../docs/reliability/testing.md); a warning printed by a test is not an assertion.
