# Trust repository guide

These instructions apply repository-wide. Each crate has a narrower `AGENTS.md`; read it as well before editing that subtree.

## Workspace and boundaries

Trust is a Rust 2021 workspace for risk-managed trading. Financial correctness, deterministic accounting, auditability, and safe broker behavior take priority over convenience.

- `model` (`trust-model`): domain entities and the `Broker` and `DatabaseFactory` contracts.
- `core`: business rules and orchestration through `TrustFacade`.
- `db-sqlite`: Diesel/SQLite implementation of the database contracts.
- `cli` (`trust-cli`, binary `trust`): Clap commands, dialogs, routing, output, and integration tests.
- `alpaca-broker`: Alpaca implementation of `Broker`.
- `ibkr-broker`: Interactive Brokers Client Portal implementation of `Broker`.
- `broker-sync`: in-memory actor/session scaffold plus serialized messages and a separate connection state machine. Real websocket reconciliation is not wired into the actor yet.
- `advisor`: catalyst, correlation, regime, and advisory configuration support.

The intended application flow is:

```text
CLI -> core::TrustFacade -> model contracts -> database/broker adapters
                         -> advisor services
```

Keep business decisions out of the CLI and adapters. The CLI constructs concrete dependencies and has narrow credential/metadata exceptions, but business mutations should go through `TrustFacade`. Broker and database crates translate external representations into model types; they do not own risk policy.

## Correctness and safety

- Use `rust_decimal::Decimal` for financial calculations. Do not introduce floating-point financial arithmetic; a few legacy model rule percentages remain `f32` boundary values.
- Do not add `unwrap`, `expect`, unchecked indexing, panic paths, lossy casts, or unchecked arithmetic to production code. Most crate roots deny these through Clippy; use checked/saturating operations and typed errors.
- Preserve validation before capital commitment and broker submission.
- Treat balances, trade state, orders, executions, fees, and distributions as one auditable system. Operations requiring multiple financial writes should be atomic and tested for rollback.
- Preserve broker/account identity checks. Broker-specific behavior belongs behind `model::Broker` and should return domain-compatible values or explicit errors.
- Never print or commit credentials, keychain values, tokens, or credential-bearing URLs.
- Do not run ignored live-broker tests unless the user explicitly requests them and has configured the environment.

## Trade lifecycle

`DraftTrade` is creation input; the persisted initial status is `New`. The status set in `model/src/trade.rs` supports:

```text
New -> Funded -> Submitted -> PartiallyFilled/Filled
                              -> ClosedTarget/ClosedStopLoss
                              -> Canceled/Expired/Rejected where valid
```

Every trade has entry, safety-stop, and target orders. Do not infer allowed transitions from this summary: preserve the validations and accounting effects in `core/src/commands/trade.rs`. Closing feeds leveling and conditional positive-profit distribution; grading is an explicit operation.

## Editing workflow

- Inspect the relevant trait, facade method, persistence worker, adapter, and tests before changing a cross-layer workflow.
- Keep changes scoped. Do not opportunistically rewrite the large CLI dispatcher or generated files.
- Never edit `db-sqlite/src/schema.rs` manually. Create paired migrations under `db-sqlite/migrations/` and use the repository migration workflow.
- Preserve existing user changes in a dirty worktree.
- Add regression tests at the lowest useful layer and integration coverage for cross-crate behavior.
- Treat CLI JSON as a public contract. Refresh snapshots only for intentional changes and inspect the diff.

## Build and verification

The toolchain is pinned in `rust-toolchain.toml`. Make targets that set up the database require:

```bash
cargo install diesel_cli --no-default-features --features sqlite
```

Use focused checks while iterating:

```bash
cargo test -p trust-model
cargo test -p core
cargo test -p db-sqlite -- --test-threads=1
cargo test -p trust-cli --test integration_test_trade -- --test-threads=1
cargo test -p alpaca-broker
cargo test -p ibkr-broker
cargo test -p broker-sync
cargo test -p advisor
```

Repository gates:

```bash
make fmt                 # write formatting changes
make fmt-check           # verify formatting
make lint                # workspace Clippy, all targets/features
make test-single         # serialize DB/CLI-sensitive workspace tests
make ci-fast             # fmt-check + lint
make ci-snapshots        # verify CLI JSON snapshots
make ci-perf             # broker-sync performance gate
make ci                  # full local CI, including coverage
```

CI also runs `cargo test --locked --no-default-features --workspace`. DB- and CLI-backed tests share process/filesystem state, so default to `--test-threads=1` for them. Run formatting and relevant tests after edits; broaden verification in proportion to the change.

## Navigation

- Domain contracts: `model/src/broker.rs`, `model/src/database.rs`
- Trade/order state: `model/src/trade.rs`, `model/src/order.rs`
- Facade and orchestration: `core/src/lib.rs`, `core/src/commands/trade.rs`
- Validation/calculation: `core/src/validators/`, `core/src/calculators_trade/`
- Persistence: `db-sqlite/src/database.rs`, `db-sqlite/src/workers/`
- CLI definition/routing: `cli/src/main.rs`, `cli/src/command_routing.rs`, `cli/src/dispatcher.rs`
- Broker adapters: `alpaca-broker/src/`, `ibkr-broker/src/`
- Sync contracts/state: `broker-sync/src/messages.rs`, `broker-sync/src/state.rs`
- Advisory analysis: `advisor/src/`

Format with `cargo fmt --all`. Follow the existing typed-error style, and make error messages actionable without exposing secrets. If asked to commit, use Conventional Commits with the affected area.
