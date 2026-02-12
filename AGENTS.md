# Repository Guidelines

## Project Structure & Module Organization
Rust workspace for the Trust trading system. Main crates:
- `model/`: domain types and core traits (`Broker`, database contracts, trade/rule models).
- `core/`: business logic (`TrustFacade`), validators, and calculators.
- `db-sqlite/`: Diesel + SQLite persistence and migrations.
- `alpaca-broker/`: Alpaca broker adapter implementation.
- `broker-sync/`: broker synchronization/state-machine logic.
- `cli/`: user-facing CLI commands, dialogs, and views.

Tests live inline (`src/*`) and in `cli/tests/` and `broker-sync/tests/`.

## Build, Test, and Development Commands
Use `make` targets to match CI behavior:
- `make build`: set up DB and build the workspace (debug).
- `make run`: run the CLI binary (`cargo run --bin cli`).
- `make test`: run workspace tests with all features.
- `make test-single`: run tests single-threaded (useful for DB-related tests).
- `make ci-fast`: formatting + clippy quick gate.
- `make ci` or `make pre-push`: full local CI parity checks.

Prerequisite: install Diesel CLI for `make setup/build/test`:
`cargo install diesel_cli --no-default-features --features sqlite`.

## Coding Style & Naming Conventions
- Format with `cargo fmt --all` (or `make fmt`); CI enforces formatting.
- Lint with `cargo clippy --workspace --all-targets --all-features -- -D warnings` (or `make lint`).
- Follow Rust naming conventions: `snake_case`, `CamelCase`, `SCREAMING_SNAKE_CASE`.
- Keep `unwrap`, `expect`, and panic-style patterns out of production code; strict clippy denies are enabled in crate roots (tests allow limited exceptions).

## Testing Guidelines
- Run `make test` before opening a PR.
- For persistence/concurrency-sensitive areas, also run `make test-single`.
- Run `cargo test --locked --no-default-features --workspace` to mirror the CI no-default-feature test lane.
- Run `make ci-test` for all-features + doc tests.
- Keep integration tests descriptive and use property tests for state/rule invariants (`broker-sync/tests/*property_test.rs`).

## Commit & Pull Request Guidelines
Recent history follows Conventional Commits:
- `feat(core): add ...`
- `fix: address ...`
- `test(cli): add ...`

Use `type(scope): short imperative summary`; include scope when changes are crate-specific. Keep commits focused and reviewable.

For PRs targeting `main`:
- link the related issue,
- summarize behavior changes and affected crates,
- include validation run (`make ci-fast` minimum; `make pre-push` preferred),
- provide CLI command/output examples when user-facing behavior changes.

## Security & Configuration Tips
- Do not commit broker credentials or secrets; use the key-management CLI flow and OS keyring.
- Local development DB defaults to `~/.trust/debug.db`; clean with `make delete-db` when needed.
- Run `make security-check` when changing dependencies or auth/broker logic.
