# Multi-Asset Completion Audit

This audit maps the active multi-asset objective to concrete Trust artifacts. It is intentionally stricter than a status summary: a requirement is only marked covered when there is code, tests, or a runnable command that directly supports it.

## Objective Restated

Trust should:

1. Keep all business logic behind `TrustFacade` and make the risk path understandable.
2. Provide stronger Interactive Brokers E2E coverage.
3. Prove core risk-management invariants are hard to break.
4. Work across stocks, bonds, ETFs, and existing non-stock categories.
5. Improve CLI UX for managing a larger securities universe.
6. Store and use bond fixed-income terms.
7. Calculate useful bond/security stats.

## Prompt-To-Artifact Checklist

| Requirement | Evidence | Status |
| --- | --- | --- |
| Understand how Trust works | `AGENTS.md`, `core/src/lib.rs`, `docs/trust-proof.md` document `TrustFacade` as the entry point and the focused proof suite. | Covered for this change set. |
| Better E2E tests using Interactive Brokers | `ibkr-broker/tests/http_integration.rs` covers mocked IBKR preflight, stock/ETF/bond submission, stock/ETF/bond `/whatif` preview, bond `BOND` mapping, and oversized plus invalid-geometry risk rejection before broker I/O. `ibkr-broker/tests/live_gateway_smoke_test.rs` covers authenticated Client Portal preflight, contract resolution, and non-mutating what-if bracket previews when enabled. | Partially covered; live authenticated run still required. |
| Live E2E readiness before broker contact | `make ibkr-live-env-check` reports all missing variables. `make ibkr-live-input-check` validates stock/ETF/bond symbols and positive long bracket geometry before preflight or what-if calls. | Covered locally; live broker run still blocked by missing env. |
| Prove risk management is strong | `core/tests/risk_invariants_test.rs` has table-driven and generated stock/ETF/bond long/short cases. It asserts exact boundary funding, one-unit-over rejection, invalid geometry rejection, and sizing-vs-funding agreement. | Covered locally by `make trust-proof`. |
| Impossible to break risk management | `.github/workflows/rust.yaml` runs `make trust-proof` in CI. `core/src/commands/trade.rs` still routes funding through core validators. `cli/tests/architecture_guard.rs` fails if production CLI trade/risk mutations bypass `TrustFacade` or call broker mutation surfaces directly. | Covered as regression protection, not as a mathematical impossibility. |
| Works with stocks, bonds, ETFs | `model/src/trading_vehicle.rs`, `db-sqlite` migrations, `ibkr-broker/src/contracts.rs`, `core/tests/risk_invariants_test.rs`, `core/src/commands/trade.rs`, and `ibkr-broker/tests/http_integration.rs`. | Covered for model, DB, risk, execution quantity safety, and mocked IBKR submission. |
| Migration upgrade/rollback safety | `db-sqlite/src/migration_fk_safety_tests.rs` covers the multi-asset category migration and fixed-income terms migration, including dependent foreign keys and rollback behavior. | Covered by `make trust-proof`. |
| Manage more securities professionally | `trading-vehicle search` supports non-interactive filters and JSON. `trading-vehicle stats` summarizes inventory by category, broker, and bond term coverage. | Covered by `cargo test -p trust-cli trading_vehicle`, `test_trading_vehicle_stats_cli_reports_multi_asset_bond_inventory`, and `test_trading_vehicle_search_cli_filters_missing_bond_terms_json`. |
| Include bond interest rates | `FixedIncomeTerms` stores face value, coupon rate, maturity, and coupon frequency. CLI create/import/update can populate those terms. | Covered by DB and CLI tests. |
| Calculate stats beyond stocks | `metrics bond` calculates coupon income, current yield, approximate YTM, accrued interest, dirty price, and dirty position value. `trading-vehicle stats` summarizes bond inventory. | Covered by core tests and CLI round trips for stored bond terms, accrued interest, dirty price, and inventory stats. |
| One-command local evidence | `make trust-proof` runs the focused proof suite. | Covered. |
| CI evidence | `.github/workflows/rust.yaml` has `Multi-Asset Risk Proof` job and `ci-success` depends on it. | Covered. |
| Migrations committed reliably | `.gitignore` explicitly un-ignores `db-sqlite/migrations/**/*.sql`; new migration folders are visible to git. | Covered; files still need to be added to the commit. |

## Verified Commands

Latest passing local evidence from this branch:

```bash
make trust-proof
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --locked --all-features --workspace
cargo test --locked --no-default-features --workspace
cargo test -p db-sqlite migration_fk_safety_tests -- --test-threads=1
cargo test -p trust-cli --test integration_test_dispatcher_commands test_trading_vehicle_stats_cli_reports_multi_asset_bond_inventory -- --test-threads=1
cargo test -p trust-cli --test integration_test_dispatcher_commands test_trading_vehicle_search_cli_filters_missing_bond_terms_json -- --test-threads=1
diesel migration run --config-file db-sqlite/diesel.toml --database-url <temp-db>
diesel migration revert --config-file db-sqlite/diesel.toml --database-url <temp-db>
diesel migration revert --config-file db-sqlite/diesel.toml --database-url <temp-db>
```

Blocked live evidence:

```bash
make ibkr-live-env-check
make ibkr-live-input-check
make ibkr-live-preflight
make ibkr-live-e2e
TRUST_IBKR_LIVE_E2E=1 cargo test -p ibkr-broker --test live_gateway_smoke_test -- --ignored --nocapture --test-threads=1
```

`make ibkr-live-env-check` fails fast in the current shell because the required account, stock, ETF, bond, and bracket-price variables are not set. `make ibkr-live-input-check` also requires those variables, but performs only no-network validation. `make ibkr-live-preflight` and `make ibkr-live-e2e` require an authenticated Client Portal Gateway and the variables documented in `docs/interactive-brokers-e2e.md`.

## Remaining Gaps

- Run `make ibkr-live-e2e` against an authenticated IBKR Client Portal Gateway with real stock, ETF, and bond symbols plus realistic bracket prices.
- Add all currently untracked source artifacts, especially the visible migration directories.
- This proof suite materially strengthens risk safety, but no test suite can prove future risk logic is impossible to break unless the suite remains required in CI and review for risk, broker, securities, and bond-analytics changes.
