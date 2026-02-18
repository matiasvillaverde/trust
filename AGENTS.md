# AGENTS.md

Guidelines for sub-agents working in this repository.

## Project

Rust workspace — risk-managed algorithmic trading system. Six crates:

- **model** (`trust-model`): Domain types + trait contracts (`Broker`, `DatabaseFactory`). All monetary values use `rust_decimal::Decimal` — float arithmetic is denied at compile time.
- **core**: `TrustFacade` is the single entry point for all business logic. Contains calculators, validators, services (leveling, distribution, grading, advisory), and commands. CLI never calls DB or broker directly.
- **db-sqlite**: `SqliteDatabase` implements `DatabaseFactory`. Diesel ORM with SQLite. Schema in `db-sqlite/src/schema.rs` is auto-generated — never edit it manually. Migrations in `db-sqlite/migrations/`.
- **cli**: Clap CLI. `cli/src/dispatcher.rs` is the central command router (~184KB). Commands in `cli/src/commands/`, views in `cli/src/views/`, dialogs in `cli/src/dialogs/`.
- **alpaca-broker**: `AlpacaBroker` implements the `Broker` trait. Each operation (submit, sync, close, cancel, modify) is in its own module.
- **broker-sync**: Actor-based real-time sync. `BrokerSync`/`BrokerSyncHandle` with command/event messaging and state machine logic.

## Build & Test

```bash
make build              # Setup DB + debug build
make test               # All tests (multi-threaded)
make test-single        # Single-threaded (required for DB/CLI tests)
make fmt                # Auto-format
make lint               # Clippy with -D warnings
make ci-fast            # fmt + clippy
make ci                 # Full CI: fmt, clippy, build, tests, snapshots, perf
```

```bash
# Specific crate/test
cargo test -p core
cargo test -p cli -- --test-threads=1
cargo test -p cli --test integration_test_trade -- test_name
```

```bash
# Snapshots & perf
make ci-snapshots       # Verify JSON report snapshots
make snapshots-update   # Regenerate snapshots (UPDATE_SNAPSHOTS=1)
make ci-perf            # Performance regression gate
```

Prerequisite: `cargo install diesel_cli --no-default-features --features sqlite`.

## Key Architecture

- **DatabaseFactory** returns specialized read/write trait objects (e.g., `AccountRead`, `OrderWrite`, `ReadTradeDB`). Uses named savepoints for atomicity.
- **Trade State Machine**: Draft → Funded → Submitted → Filled → Closed (ClosedTarget/ClosedStopLoss) or Canceled. Every trade has three orders: entry, stop-loss, target.
- **Risk Validation Flow**: CLI → Core Validators → DB Check → Broker API → DB Update. Validation before capital commitment.
- **Event-Driven**: Trade close triggers leveling evaluation, profit distribution, and grading.
- **Protected Mode**: Argon2 password authorization for critical mutations.

## Testing

- **CLI integration tests**: `cli/tests/` (18 files). Must run single-threaded (`--test-threads=1`).
- **Core tests**: `core/src/integration_tests.rs` with mocks in `core/src/mocks.rs`.
- **DB tests**: `db-sqlite/src/migration_fk_safety_tests.rs` for FK constraint validation.
- **Broker-sync tests**: `broker-sync/tests/` including property tests (`*property_test.rs`).
- **Snapshot tests**: JSON contract tests for CLI reports. Regenerate with `make snapshots-update`.
- Run `cargo test --locked --no-default-features --workspace` to mirror CI no-default-feature lane.

## Code Style

- Format: `cargo fmt --all` (CI enforced).
- Lint: `cargo clippy --workspace --all-targets --all-features -- -D warnings`.
- No `unwrap`/`expect` in production code — strict clippy denies in crate roots.
- Conventional Commits: `feat(core): add ...`, `fix(cli): resolve ...`, `test(broker-sync): add ...`.

## Where to Find Things

- Risk validation: `core/src/validators/`
- Trade calculations: `core/src/calculators_trade/`
- Trade lifecycle: `core/src/commands/trade.rs`
- CLI command routing: `cli/src/dispatcher.rs`
- CLI command definitions: `cli/src/commands/`
- Database queries: `db-sqlite/src/database.rs`
- Broker operations: `alpaca-broker/src/` (one module per operation)
- Domain models & traits: `model/src/`
- DB schema changes: Create migration with `make migration NAME=x`, edit `db-sqlite/migrations/`, then `make build`
