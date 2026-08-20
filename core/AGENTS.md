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

Runnable coverage is primarily inline module tests, `src/security_tests.rs`, and `tests/risk_invariants_test.rs`. `src/integration_tests.rs` is currently not declared by `lib.rs`; do not assume it runs.

```bash
cargo test -p core
cargo test -p core --test risk_invariants_test -- --test-threads=1
make trust-proof
```

Use mocks in `src/mocks.rs`. Add property tests for general financial invariants and regression tests for every lifecycle failure mode changed.
