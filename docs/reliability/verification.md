# Reliability verification

This work inventories **82 service tools (35 mutations)** and **192 edge-case register items**. Those inventory counts are not passed-test counts. Some register items describe multiple boundaries; a few require a broker paper environment or a real deployment. No finite test suite proves that risk management is infallible or that a stop guarantees a maximum loss.

## Repeatable offline gate

```bash
make reliability
cargo test --locked --no-default-features --workspace -- --test-threads=1
cargo clippy --locked --workspace --all-features --all-targets -- -D warnings
cargo fmt --all --check
python scripts/reliability.py --coverage
make ci-perf
docker build -t trust-service:local .
python scripts/container_smoke.py
```

`make reliability` runs the entire all-features workspace, including legacy CLI contracts, serializing state-sensitive tests. It writes `.local/reliability/results.log` and `.local/reliability/results.json`. The JSON contains every executed test name, its suite, result, ignored-test reasons, Cargo.lock digest, and required-regression checks. The gate rejects incomplete output, a nonzero cargo exit code, a failed or missing required regression, and drift between the documented tools and the authoritative catalogue. CI also checks the inventory/parser; the Rust workspace test jobs run the new regressions.

The register is a review checklist. The report is evidence of executed assertions. Neither matching a test name nor high line coverage establishes every interpretation of an edge-case item.

Coverage requires LLVM tools matching the Rust compiler. Install the pinned toolchain's `llvm-tools-preview` component, or set `LLVM_COV` and `LLVM_PROFDATA` to matching installed binaries for that command. The coverage gate retains the repository thresholds: 94% lines, 90% functions and 93% regions. It includes instrumented test code and excludes the legacy `test_support.rs` fixture module; it is not branch coverage or a financial correctness proof. Keep DB/CLI suites and the performance gate sequential, since some older tests share filenames.

## Reproduced defects and regression evidence

| Defect reproduced | Change | Direct evidence |
| --- | --- | --- |
| SQLite changed a configured 0.1% cap into 0% | Persist decimal-string percentages, retain legacy integer compatibility, reject unsupported precision and out-of-range values | `fractional_risk_rules_round_trip_and_preview_is_fundable`; `invalid_rule_percentages_are_rejected_without_writes`; migration and backup tests |
| Invalid stop geometry could be funded without a per-trade rule | Validate positive bracket prices, side, quantities and target geometry independently of rule presence | `funding_geometry_is_enforced_even_without_risk_rules` |
| A monthly-only policy did not constrain quantity | Resolve rules by type, enforce monthly-only budget, use the tightest duplicate cap independently of priority ordering | `monthly_rule_alone_limits_risk_and_short_collateral_preview`; quantity unit tests |
| Funded short collateral hid planned loss; submitted/filled trades disappeared from monthly risk reads | Query all active states; derive planned liquidation equity from retained cash, remaining exposure and pending entry orders | `monthly_risk_counts_long_and_short_through_active_states`; `open_risk_query_includes_only_active_states_and_correct_account_currency`; `partial_entries_exits_and_cancelled_remainders_preserve_planned_equity` |
| A deposit or returned carried collateral could look like a gain renewing risk | Adjust the UTC monthly ledger reference for capital flows and carried reservations | `monthly_budget_does_not_reset_after_current_month_deposit_or_carried_reservation` |
| Short previews could exceed stop collateral; level 4 previews could exceed a separately enforced base cap | Align preview collateral and cap final quantity at the enforceable base ceiling | `preview_never_exceeds_fundable_base_at_any_level`; short-preview boundary regression |
| Manual tools had no actual execution price; manual fills did not persist cumulative order execution state | Require evidenced actual price and persist fill quantity, status and average price inside the financial savepoint | `manual_tools_record_actual_prices_and_conserve_long_short_cash`; `generated_lifecycles_conserve_cash_with_fees_and_adverse_exits` |
| Read failures could become an empty portfolio; partial exits could disappear from open-position reports | Propagate errors and retain partially filled positions | `read_failures_never_look_like_an_empty_portfolio`; `partial_positions_remain_visible_after_an_exit_transaction` |
| Account transaction reads omitted non-USD balances, defeating mixed-currency checks | Read every supported currency and reject implicit aggregation | `mixed_currency_ledgers_remain_visible_and_cannot_be_aggregated` |
| Monetary input/restore parsing could round excess precision | Require exact Decimal parsing | Complete catalogue boundary matrix; `fractional_risk_backup_round_trip_and_corruption_rejection` |
| Order reads silently clamped negative quantities and discarded malformed optional prices | Reject negative, overfilled, malformed or unrepresentable persisted values; validate writes before SQL | `order_sqlite_conversion_rejects_corrupt_financial_values` (25 corrupt-field cases); `invalid_order_writes_leave_persisted_values_unchanged` |
| Two database connections could exit during schema inspection before a busy timeout was installed | Configure the SQLite timeout before the first schema read | Existing competing-submit regression reproduced `database is locked`, then passed after the fix |
| Some older risk tests logged a bug but still passed | Replace warnings with assertions for invalid and duplicate risk rules; preserve zero as an intentional pause | `bug_040`, `bug_041`, `bug_119`–`bug_123` in the legacy bug-hunt suite |

The initial targeted run failed five assertions and passed the rollback tests; subsequent read-failure and connection-race reproductions also failed before their fixes. Regression failures are assertions about behavior, not deliberately weakened snapshots.

## Additional failure testing

- **Real persistence failures:** abort SQLite writes at transaction insert, trade/order update, account projection and trade projection boundaries. Compare every exported financial table before/after rejection. Cover long/short funding, deposits, withdrawals, manual entry/target/stop and funded cancellation. Existing distribution tests fail individual allocation legs and history insertion.
- **Generated money conservation:** 128 deterministic generated long/short lifecycles with quantities, actual exits and fees; assert final cash equals original cash plus signed P/L minus fees, zero remaining position capital, persisted cumulative fills and no second booking from a stale fill.
- **Risk properties:** existing stock/ETF/bond long/short exact-boundary and oversized cases plus generated valid/invalid geometry. Fractional percentages now run through real SQLite and the facade rather than a calculator-only stub.
- **All 82 tool contracts:** 3,168 counted field-boundary rejection assertions, plus valid examples, unknown top-level arguments and non-object requests. Cover each required field omitted, wrong types, integer/string/array boundaries, malformed identifiers, decimal overflow and excess precision. Each tool denies access when its scope is absent from either token or operator grant, before domain/broker access.
- **Concurrency:** 32 concurrent identical deposit requests produce one operation and one credit. Sixteen distinct withdrawals competing for insufficient total cash allow one debit. Existing competing broker submission/modification tests retain one external attempt.
- **Crash boundaries:** failure before journal intent prevents financial dispatch; failure after a committed deposit but before journal completion stops the worker. Restart returns `outcome_unknown`, with one ledger entry and no automatic replay. Existing intent-only recovery, account-revocation and audit-tampering tests remain enabled.
- **Service workflows:** authenticated HTTP/MCP discovery, resources/prompts, live/account/scope gates, long/short manual lifecycle, report variants, operation replay, storage ownership and shutdown readiness.
- **Migration/recovery:** preserve existing integer rules, round-trip fractions through backups, reject malformed/range/precision-corrupt policies before restore, verify FK integrity, refuse lossy downgrade and permit safe integer downgrade/re-upgrade.
- **Corrupt order data:** reject negative/overfilled quantities and invalid exact decimals in all six numeric order fields. Missing legacy optional fields remain supported. Invalid creates, updates and repricing leave stored orders unchanged. Existing corrupt rows require an explicit repair from trustworthy execution evidence; reads no longer conceal them.

## Evidence by feature family

| Register | Main executable evidence |
| --- | --- |
| RISK, MONTH | `core/tests/reliability.rs`, `core/tests/risk_invariants_test.rs`, core quantity/risk/funding unit modules, CLI risk/level integration suites |
| CASH, STATE | Core command/validator modules; `core/tests/security_regressions.rs`; CLI account/trade/transaction/stale-state suites |
| SYNC | Core reconciliation/security regressions; `ibkr-broker/tests/http_integration.rs`; Alpaca sync/order/fee/execution modules; SQLite execution/fee worker tests |
| ATOMIC | New full-table rollback matrix; `db-sqlite/tests/test_distribution_atomicity.rs`; distribution worker rollback tests; service journal boundary tests |
| SERVICE, AUTH, TOOLS | `service/tests/service_contract.rs`; journal compatibility tests; `cli/tests/service_client.rs`; adapter identity/redaction/security suites |
| POLICY | Core leveling/grading/distribution/advisory modules; model validation; SQLite policy workers; legacy grade/level/distribution integration suites |
| ANALYSIS | Core financial calculator modules; advisor catalyst/correlation/regime tests; report/metric/session/bias/tax integration suites and JSON snapshots |
| OPS | Database backups/migrations/workers; broker HTTP/security suites; broker-sync state/property/actor tests; Python workflow checks; container smoke |

## Explicit limits and external acceptance

- Monthly budgeting uses the ledger's original entry basis for carried positions. It is not a mark-to-market monthly drawdown limit against IBKR net liquidation value. Open partial exits and prior-month trading gains need an explicit broker-equity policy if that different guarantee is required.
- Accounts with no risk rules retain capital/geometry/level constraints but have no configured per-trade or monthly loss cap. A zero-percent rule blocks new risk. Advisory output does not enforce a funding cap.
- Planned stops can slip, fail to route, or gap. Automated fixtures cover accounting/rejection behavior, not guaranteed market execution. Exchange halts, corporate actions, borrowing/margin changes and real bracket routing require separate broker evidence.
- Ignored live IBKR tests require a configured gateway and explicit authorization. This task does not run them or place broker orders. The ignored local performance gate is a separate command, not a live test.
- Host power failure, storage exhaustion/corruption, disaster restore from off-host copies, prolonged load and actual OAuth rotation/TLS configuration require deployment drills. Injected SQLite write errors and local container restart do not prove those hardware/network properties.
- The legacy bug-hunt suite still includes exploratory tests outside the strengthened risk cases. A printed diagnostic is not counted as proof that the reported behavior is correct. No source file is counted merely because it contains an undeclared test module.
- The runtime journal and financial ledger are separate databases; external broker acceptance is a third commit boundary. Unknown outcome is intentionally preserved across those boundaries. The service does not claim distributed exactly-once execution.

## Recorded run

Recorded on **2026-10-26**, with Rust **1.99.1**. The functional/coverage gate passed; **the performance gate did not**. This is not a fully green `make ci` result.

| Check | Observed result |
| --- | --- |
| All-feature workspace with coverage, serialized tests | **2,477 passed, 0 failed, 6 ignored**; all 21 required regressions present |
| Workspace with default features disabled, serialized tests | **1,797 passed, 0 failed, 5 ignored** |
| Coverage | **95.21% lines, 90.39% functions, 94.01% regions**; existing thresholds passed |
| Tool field-boundary matrix | **3,168 rejection assertions** across all 82 tools, plus successful examples and scope-denial checks |
| Formatting, workspace/all-target Clippy, workflow checks and evidence-parser checks | Passed |
| Dependency audit, policy/license/source checks and unused-dependency check | Passed; dependency audit reported no advisories |
| Final container build and runtime smoke | Passed: non-root/read-only operation, authentication denial, exact catalogue, MCP, fractional risk persistence, restart/replay and audit verification |
| Final container vulnerability scan | **0 reported vulnerabilities** |
| Debug performance gate | **Failed:** annual workload median **47,670 ms** versus **26,000 ms**; 1,200-lifecycle workload **71,229 ms** versus **6,000 ms** |
| Release performance gate | Annual workload passed at **7,009 ms**; 1,200-lifecycle workload **failed at 10,874 ms** versus **6,000 ms** |

Both performance runs used three measured iterations after one warm-up, with the existing thresholds. Correctness assertions in those workloads passed; their timing assertions failed. The release measurement does not erase the debug failure. No comparison against an unchanged baseline on this host was completed, so these measurements do not establish which changes introduced the timing shortfall.

A temporary, single-run stage measurement of the release lifecycle workload observed approximately **0.226 s creating**, **2.542 s funding**, **0.509 s submitting**, and **8.029 s synchronizing** 1,200 trades. Synchronization is the first profiling target, followed by funding/history scaling. That measurement identifies stages, not a proven inner-function cause. The temporary instrumentation was removed; financial validations and timing thresholds remain intact. Performance remains open work.

The six ignored all-feature tests comprise five external IBKR/gateway checks and the separately executed local performance gate. No ignored broker test was enabled. The minimal configuration omits the feature-gated legacy CLI gateway test.

Repeat the optimized timing measurement with:

```bash
cargo test --locked --release -p trust-cli --test integration_test_use_cases \
  -- test_case_212_perf_gate_sync_lifecycle_regression_guard --ignored --nocapture
```

Local evidence lives in `.local/reliability/`: `results.json` and `results.log` retain every named test, `coverage.json` contains the coverage report, `no-default.json` records the alternate configuration, and `verification-status.json` explicitly records that **not all checks passed**. The performance, security, container and schema logs are retained alongside them. These generated files are intentionally ignored by Git and can be regenerated; the feature/edge-case register and regression tests are repository files.

The closeout source review uses the `autoreview` skill's default blocking-issue filter:

```bash
/home/mati/.agents/skills/autoreview/scripts/autoreview --mode local \
  --prompt-file .local/trust-reliability-review-scope.txt \
  --output /tmp/trust-reliability-review.md \
  --json-output /tmp/trust-reliability-review.json
```

Its completed output is retained as `.local/reliability/review.md` and `review.json`. Source review is additional evidence; it does not override the failed timing gate or the external acceptance limits above.
