# Trust Multi-Asset Proof Map

This document maps the current multi-asset risk and broker evidence to runnable commands. It is not a mathematical proof that no future code change can introduce a bug; it is the focused regression suite for the invariants Trust must preserve before stock, ETF, or bond execution reaches a broker.

## One Command

```bash
make trust-proof
```

The target runs these checks with `--locked` where Cargo supports it:

| Area | Command | Evidence |
| --- | --- | --- |
| Core risk invariants | `cargo test -p core --test risk_invariants_test --locked -- --test-threads=1` | Generated and table-driven stock/ETF/bond scenarios prove the funding gate accepts the exact risk boundary and rejects one unit above it through `TrustFacade` and SQLite. |
| Bond analytics | `cargo test -p core calculators_fixed_income --locked` | Decimal-only coupon income, current yield, approximate YTM, accrued interest, dirty price, and invalid schedule validation. |
| Execution quantity safety | `cargo test -p core fractional_qty_policy --locked` | Stock, ETF, and bond executions reject fractional quantities unless the vehicle explicitly allows fractional trading. |
| Security persistence | `cargo test -p db-sqlite worker_trading_vehicle --locked -- --test-threads=1` | SQLite stores multi-asset categories and bond fixed-income terms. |
| Migration safety | `cargo test -p db-sqlite migration_fk_safety_tests --locked -- --test-threads=1` | Multi-asset and fixed-income migrations preserve dependent foreign keys, accept stock/ETF/bond categories on upgrade, and roll back without dangling FK definitions. |
| CLI risk guards | `cargo test -p trust-cli --test integration_test_risk_guards --locked -- --test-threads=1` | CLI-facing long/short risk boundaries, invalid geometry, and protected risk-profile changes. |
| CLI architecture guard | `cargo test -p trust-cli --test architecture_guard --locked` | Production CLI trade/risk mutations must route through `TrustFacade`; direct broker calls are limited to metadata/credential setup; direct SQLite construction stays at dispatcher import/export boundaries. |
| Securities management UX | `cargo test -p trust-cli trading_vehicle --locked -- --test-threads=1` | Non-interactive security search filters by category, broker, symbol, ISIN, and incomplete bond terms; inventory stats summarize categories, brokers, and bond fixed-income coverage. |
| Bond UX round trip | `cargo test -p trust-cli --test integration_test_dispatcher_commands --locked test_bond_terms_update_and_metrics_cli_round_trip -- --test-threads=1` | Protected bond-term update followed by JSON bond metrics calculation from stored terms, including accrued interest, dirty price, and dirty position value. |
| Inventory stats CLI round trip | `cargo test -p trust-cli --test integration_test_dispatcher_commands --locked test_trading_vehicle_stats_cli_reports_multi_asset_bond_inventory -- --test-threads=1` | User-facing JSON stats report stock/ETF/bond counts, broker counts, complete versus incomplete bond terms, coupon coverage, average coupon rate, and maturity range. |
| Security search CLI round trip | `cargo test -p trust-cli --test integration_test_dispatcher_commands --locked test_trading_vehicle_search_cli_filters_missing_bond_terms_json -- --test-threads=1` | User-facing JSON search filters a mixed stock/ETF/bond inventory down to bonds missing fixed-income terms and includes stored bond metadata. |
| Mocked IBKR E2E | `cargo test -p ibkr-broker --test http_integration --locked -- --test-threads=1` | Trust rejects oversized and invalid-geometry IBKR stock/ETF/bond trades before broker I/O, submits valid stock/ETF/bond bracket orders, previews stock/ETF/bond bracket payloads through `/whatif`, and maps bond orders to IBKR `BOND`. |
| Live E2E compile and input gate | `cargo test -p ibkr-broker --test live_gateway_smoke_test --locked` | Ensures the ignored live Client Portal preflight, contract, and what-if preview smoke tests compile, and that the no-network long-bracket input validator is covered. The ignored tests do not contact IBKR unless run with `make ibkr-live-preflight` or `make ibkr-live-e2e`. |

## Risk Invariants Covered

- All funding goes through `TrustFacade::fund_trade`.
- CLI does not bypass core risk validation.
- The risk formula is enforced for stock, ETF, and bond trading vehicles.
- Long risk uses `entry - stop`; short risk uses `stop - entry`.
- Invalid stop/entry geometry is rejected before capital is committed.
- One unit above the calculated risk boundary is rejected.
- Oversized and invalid-geometry IBKR trades fail before any HTTP broker request.

## Live IBKR Evidence

Run the authenticated live smoke test separately:

```bash
make ibkr-live-preflight
make ibkr-live-e2e
```

See `docs/interactive-brokers-e2e.md` for required environment variables. `make ibkr-live-input-check` validates symbols and long bracket prices without contacting IBKR. A passing preflight proves the configured Client Portal Gateway is authenticated and can select the account. A passing live E2E run proves it can resolve representative stock, ETF, and bond contracts, that Trust maps them to the expected IBKR security types, and that IBKR accepts Trust's bracket payloads through non-mutating what-if previews.

## Remaining Limits

- The live IBKR smoke test does not place real or paper orders; it relies on IBKR `/whatif` previews for live order-shape evidence.
- The proof suite does not replace full `make ci`; it is a focused risk and multi-asset evidence set.
- The claim that risk management is "impossible to break" depends on this suite staying mandatory in review and CI for changes touching risk, securities, broker submission, or bond analytics.
