# Reliability verification

This work inventories **82 service tools (35 mutations)** and **212 edge-case register items**. Those inventory counts are not passed-test counts. Some register items describe multiple boundaries; a few require a broker paper environment or a real deployment. No finite test suite proves that risk management is infallible or that a stop guarantees a maximum loss.

## Repeatable offline gate

```bash
make reliability
cargo test --locked --no-default-features --workspace -- --test-threads=1
cargo clippy --locked --workspace --all-features --all-targets -- -D warnings
cargo fmt --all --check
python scripts/reliability.py --coverage
make ci-perf  # release profile, unchanged 26-second / 6-second budgets
make e2e      # real child process + HTTP/MCP + loopback IBKR simulator
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

## Previous baseline before observability and performance changes

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

The table above preserves the previous baseline. Current evidence and results are recorded below; baseline timing failures are not the result of the current release gate. Generated reports are ignored by Git and can be regenerated; the feature register, edge-case register and tests are repository files.

The closeout source review uses the `autoreview` skill's default blocking-issue filter:

```bash
/home/mati/.agents/skills/autoreview/scripts/autoreview --mode local \
  --prompt-file .local/reliability/observability-scope.md \
  --output /tmp/trust-observe-review.txt \
  --json-output /tmp/trust-observe-review.json
```

Source review is additional evidence; it does not replace execution tests or the external acceptance limits above.


## Current service hardening run

Recorded on **2026-10-26** with Rust **1.99.1**. All checks listed below passed.

The service now records request/operation correlation, source, affected accounts, timing and safe error codes in audit v2, including replays and crash recovery. Audit reads enforce every affected account grant. Authenticated `/metrics` remains available outside the financial queue, and health probes do not consume ordinary request limits. Queued work that expires cannot later execute; active work retains conservative unknown-outcome handling. SIGTERM stops admission and finishes the active operation.

Reconciliation applies exact account projection deltas inside the existing savepoint instead of replaying the entire account ledger after each snapshot. Funding still calculates available capital from ordered ledger movements, using a smaller SQLite projection. File-backed differential tests compare long/short partial fills, cancellations, exits, late fees and replays against a full ledger calculation, including injected rollback failures. These tests also exposed and fixed nested transaction failures in fee, distribution and transfer workers. Typed funding errors now cross the facade boundary without being wrapped into an uncertain result.

| Check | Current result |
| --- | --- |
| All-feature workspace with coverage | **2,489 passed, 0 failed, 6 ignored**; all **33** required regressions present |
| Coverage | **95.42% lines, 90.51% functions, 94.22% regions**; existing thresholds passed |
| Real process E2E | **5 passed**: actual HTTP/MCP, generated JWTs, real IBKR adapter with a loopback gateway simulator, partial/final fills, concurrent replay, risk rejection, process kill/restart, SIGTERM, bounded queue, journal faults, telemetry and secret redaction |
| Minimal-feature workspace | **1,809 passed, 0 failed, 5 ignored**; all **33** required regressions present |
| Release performance gate | **Passed** with 3 measured iterations and 1 warmup: annual workload median **5,245 ms** (limit **26,000 ms**); 1,200 lifecycles median **3,971 ms** (limit **6,000 ms**) |
| Debug diagnostic | 1,200 lifecycles **13,270 ms**; the production performance gate now explicitly uses `--release`, retaining the existing budgets |
| Formatting / Clippy / evidence parser | Passed; workspace Clippy covers all features and targets |
| Dependency security | Cargo audit and Cargo deny passed |
| Rebuilt hardened container | Smoke passed: non-root/read-only operation, JWT enforcement, MCP, exact catalogue, fractional risk persistence, restart/replay, audit v2 and authenticated metrics |
| Container vulnerability scan | **0 high/critical findings**; Rust dependencies are checked separately by Cargo audit |
| Source review | Default blocking review (`autoreview --mode local`) passed with no accepted/actionable findings |

The 1,200-lifecycle release median fell from **10,874 ms to 3,971 ms** (about **63% less elapsed time**). These are measured workload results, not throughput guarantees for a real broker or a different host. Broker network latency remains serialized by the financial worker.

Current generated evidence uses the `observability-` prefix under `.local/reliability/`: `observability-results.json/log`, `observability-no-default.json/log`, `observability-perf-gate.log`, container/security logs and `observability-review.txt/json`. The coverage report is `coverage.json`; `observability-status.json` records the combined outcome. The standard `results`, `no-default`, and `verification-status` files also point to this completed run. The standalone E2E suite is `make e2e`; `make reliability` requires its named regressions. Live gateway/exchange behavior, host power loss, real storage exhaustion and deployment identity-provider rotation remain external acceptance work. No real broker orders were placed.

## Staged IBKR assurance

The assurance workflow adopts three practices from inspected upstream projects: [Lumibot's durable intent and unknown-acknowledgment tests](https://github.com/Lumiwealth/lumibot/blob/dev/tests/test_order_decision_journal.py), [LEAN's separate credentialed brokerage harness](https://github.com/QuantConnect/Lean/blob/master/Tests/Brokerages/BrokerageTests.cs), and [NautilusTrader's checks that denied orders never reach execution](https://github.com/nautechsystems/nautilus_trader/blob/13a379ef/tests/unit_tests/risk/test_engine.py). Trust implements these patterns within its own Rust domain and Client Portal adapter. LEAN/Nautilus TWS payloads are not Client Portal contracts. No upstream tests certify Trust.

| Level | Evidence | Entry point | Limit |
| --- | --- | --- | --- |
| 1 — Deterministic | Current-policy admission, reservation boundaries, generated accounting/risk properties, persistence and permissions | `make assurance-unit` | Local ledger and controlled broker doubles |
| 2 — Protocol/process | Documented HTTP response shapes, snapshot completeness, real process HTTP/MCP, crash/restart and concurrent requests | `make assurance-protocol` | Loopback IBKR simulator |
| 3 — Paper preflight | Broker-confirmed paper identity, contract resolution and what-if | `make ibkr-paper-preflight` | Real gateway; no submitted orders |
| 4 — Paper lifecycle | Supervised MCP orders, fills, protection, cancellation and independent account reconciliation | `scripts/ibkr_paper_acceptance.py` supplies one submit/cancel smoke | Smoke success is insufficient to pass this level |
| 5 — Operational | Multi-session recovery, external positions, snapshot freshness, account-wide limits, alerting, halt and restore | Acceptance matrix and operator evidence | Remains unverified; additional whole-account controls are required |
| 6 — Live canary | Separately authorized, bounded real-money validation after all earlier gates | Operator-supervised acceptance only | Never automatically enabled or executed by CI |

`make assurance-status` reports evidence against a fingerprint of the current tracked and untracked source, including the dirty worktree. Each automated level saves exact commands, process exit codes, named test results, a log hash, timestamp and source fingerprint in `.local/assurance/level-N.json`. Required tests that are missing, ignored, failed, or truncated cannot pass. Changes to source, altered logs, future timestamps and evidence older than seven days require a new run. Seven days is an operational freshness rule, not a statistical confidence claim. These files are local operator evidence, not signed attestations or runtime authorization. The runner never executes levels 4–6 or marks them passed from a smoke result.

The [52-case acceptance matrix](assurance.json) retains the broader scenarios from the investigation. Its proposed cases are requirements, not a count of completed tests. Offline levels cover their explicitly stated scopes; they do not imply complete whole-account broker reconciliation. Current service balances are local projections, monthly risk is planned ledger-basis loss rather than mark-to-market drawdown, and reconciliation failures do not yet impose a durable account-wide halt. These remain blockers for autonomous live readiness. Risk-reducing operations and protective stops require deliberate treatment during an emergency halt.

Two investigation failures are now permanent regressions:

- A funded trade must revalidate current caps, level and ledger capital before submission intent. Its own reservation is credited once for calculation, while other reservations remain. Tests tighten trade/monthly caps for longs and shorts, check unchanged/full-reservation boundaries, compare all financial tables after denial, corrupt reservation/rule reads, and assert zero broker calls. The generated risk-boundary test now proceeds through submission.
- `force=true` on IBKR's order endpoint clears cached information and returns an empty array; it is not an empty account snapshot. The adapter reads with `force=false` and requires `snapshot=true`, an orders array, and fewer than the documented 1,000-order maximum. Missing/incomplete/truncated evidence prevents a new POST. Both HTTP fixtures and process simulator distinguish cache refresh from snapshot reads. See [IBKR's contract](https://www.interactivebrokers.com/docs/web-api/api-reference/trading/trading-orders/get-open-orders).

Service funding/submission now requires both per-trade and monthly rules. The independent `trust:manual` scope protects manual execution records; `trust:reconcile` retains broker-driven reconciliation. Existing owner tokens and grants need explicit `trust:manual` when manual accounting is intended. Trading agents should not receive policy, manual-accounting or administration authority.


`make reliability` also records levels 1 and 2 from the same complete workspace run, avoiding repeated test execution. `--summarize` does not promote historical logs. Property tests record `PROPTEST_RNG_SEED` (default `20261027`); set another seed for additional exploration. `make assurance-mutations` copies source into an isolated temporary workspace and removes three selected guards: submission policy revalidation, correct IBKR snapshot lookup, and manual-accounting scope separation. Each experiment first verifies its baseline test, then requires an executed failing assertion in the mutant. Compiler failures do not count as success. This is a focused sensitivity check, not an exhaustive mutation score.

The isolated mutation workspace also checks source completeness. It exposed a service control-database migration hidden by a machine-wide `*.sql` ignore rule. The repository now explicitly includes `service/migrations/**/*.sql`, alongside the existing Diesel exception, so a clean source checkout can compile and initialize the durable operation journal. Compile failures remain invalid mutation experiments and must be repaired before claiming a killed guard.
