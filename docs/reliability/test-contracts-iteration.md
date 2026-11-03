# Financial test contracts: second iteration

This follows the [original candidate cleanup](test-usefulness-improvements.md). The historical usefulness catalog remains a snapshot, not a claim that every indexed test was individually reviewed. This iteration addresses the eight remaining families identified in the follow-up review.

| Family | Executable contract |
| --- | --- |
| Risk-limit rejection | With $50,000 cash, a 2% per-trade budget and valid unrelated policies, long and short proposals risking $999.98 and $1,000 reserve exactly the required capital. A $1,000.02 proposal returns `RiskPerTradeExceeded`, preserves every financial table and never calls a broker. The instrument explicitly supports fractional quantities. |
| Invalid drafts | Seventeen permissive tests are consolidated into four contracts covering nonpositive quantities, negative prices at each order-creation boundary, invalid bracket geometry and unaffordable large quantities. Snapshots precede the operation. Draft rejection leaves no orphan orders; rejected funding leaves the editable draft and all financial rows unchanged. Valid neighboring proposals must fund successfully. |
| Lifecycle and portfolio | The 120 numeric lifecycle variations become 64 named combinations: long/short, whole/fractional, requested/improved entry, with/without fees, and target/improved target/stop/gap beyond stop. Six named portfolio contracts cover winners, losing streaks, recovery, alternating outcomes, small gross wins erased by fees, and entry-price refunds. |
| Settlement | Fixed clocks exercise all five supported asset categories across weekends, month/year boundaries and leap day. Expectations are explicit dates, followed by persisted readback. Normal public creation separately checks an independent weekday-offset table. A private timestamp parameter makes the persistence clock deterministic without changing the public API. |
| Broker-double self-test | The dispatcher test that directly called six fake methods is removed. Broker failure behavior remains covered by facade/SQLite contracts in `integration_test_broker_failures`, including uncertain cancellation preserving reservations. |
| Calculator errors | Six expected-panic tests now inspect the returned errors. An unexpected calculator panic fails normally. These remain fast unit tests. |
| Workload separation | Default tests exercise 36,865 analytical records with exact statistics and 32 actual persisted lifecycle sequences. The opt-in release performance gate measures analytical aggregation and 1,200 actual broker-sync lifecycles separately. A call counter verifies every lifecycle reaches the broker adapter boundary. |
| Release workflow | Parsed YAML assertions check triggers, validation dependencies, version guards, permission scope, all remote action pins and the platform matrix. A separate acceptance test uses the workflow's archive/checksum commands on a real compiled CLI, runs the actual installer with local download substitution, executes the installed binary, and rejects a modified archive. CI runs both layers. |

The lifecycle expectations derive directional gross P&L from the proposed scenario's actual entry and exit prices, then subtract explicit entry/exit fees. They verify reservation amounts, refunds, every cash receipt, account cash, trade P&L, protective-order terminal states, and repeated reconciliation. The expected cash curve is constructed from these inputs, not from the produced ledger. Drawdown assertions use this independently specified curve.

Two existing semantics remain explicit: the legacy `average_r_multiple` report uses planned bracket prices, while realized P&L includes actual prices and fees; and the legacy drawdown calculator describes available-cash movements, including reservations, rather than mark-to-market broker equity. Settlement defaults count weekdays only, not exchange holidays. These tests do not broaden those product contracts silently.

The risk-enabled performance workload exposed unnecessary closed-history scans in monthly risk calculations. Carried-capital lookups now run only when older funding/payment rows exist; current-month deposits and withdrawals are still applied. When closed positions must be examined, closing-receipt IDs are indexed once instead of repeatedly searching the ledger. Existing month-boundary contracts verify open and closed carried positions and new deposits.

All new test modules stay below the 1,000-line architecture limit. Existing allowances for the affected large files are reduced; no coverage thresholds or exclusions are relaxed.

## Verification

Named outcomes and logs for this iteration are retained under `.local/contract-iteration-20261101/`. Run:

```bash
cargo fmt --all -- --check
python3 scripts/check_architecture.py --base HEAD
cargo clippy --workspace --all-targets --all-features -- -D warnings
python3 scripts/reliability.py --coverage --output .local/contract-iteration-20261101/validated.json
cargo test --locked --no-default-features --workspace -- --test-threads=1
python3 scripts/mutation_gate.py
python3 -m unittest discover -s scripts -p 'test_*.py'
python3 -m pip install --require-hashes -r .github/tests/requirements.txt
bash .github/test_version_parsing.sh
cargo build --locked --release --bin trust
python3 tests/test_release_artifact.py
cargo test --locked --release -p trust-cli --test integration_test_use_cases \
  -- performance::analytics_and_1200_persisted_lifecycles_meet_performance_budgets --ignored --nocapture
```

The performance gate uses three measured iterations after a warm-up. Defaults are 1,000 ms for 36,865 analytical records and the existing 6,000 ms budget for 1,200 persisted sync lifecycles. The workloads changed, so older combined-workload timings are not directly comparable. The 2026-11-01 local run measured 34.4 ms for analytics and 17.764 seconds for the 1,200 risk-enabled lifecycles. That run **failed** the unchanged six-second limit. Removing redundant carried-capital lookups had improved the same workload from 50.529 seconds. The subsequent [instrumented optimization](performance-instrumentation.md) identified repeated ledger reads, reduced the same gate to a 4.792-second median and passed the unchanged limit; the earlier failure remains part of the verification record. Override `TRUST_PERF_GATE_MAX_ANALYTICS_MS` and `TRUST_PERF_GATE_MAX_SYNC_MS` only for an explicitly documented environment-specific budget; a default-budget failure must remain visible.

The mutation register adds seven compiled experiments: excluding the exact risk boundary, removing the per-trade guard, disabling partial-order rollback, using the wrong settlement offset, omitting a closing fee, reporting zero drawdown, and dropping carried monthly capital. A passing unmodified baseline and failing named test are required; compilation errors or selecting zero tests do not count as detection.

All broker activity here uses controlled adapters. The artifact acceptance covers the host platform; it does not establish macOS cross-compilation on Linux. Real IBKR paper acceptance and native credential-store acceptance remain separate work.
