# Test quality audit: 2026-10-31

## Inventory and review scope

The starting revision, `f79fd7f`, registered 2,552 Rust tests under `cargo test --workspace --all-features -- --list` (nine ignored). A Rust syntax-tree inventory found 2,400 ordinary test functions in 274 files: 1,651 under `src/` and 749 in integration trees. Macro-generated tests and conditional compilation account for the difference; the source count is not the executed count.

The audit used the registered suite, a per-function assertion/entry-point index across every crate, and detailed body/production-path review of weak candidates and their replacements. Counts of assertions, names such as `default`, or placement under `src/` do not determine quality. This is a repository-wide inventory and targeted contract review, not a claim that every assertion has an independent correctness proof.

| Area | Ordinary tests under `src/` before changes | Assessment |
| --- | ---: | --- |
| Core | 489 | Preserve Decimal formulas, financial boundaries, leveling and valid/invalid lifecycle pairs. Many inline tests already integrate real SQLite. Remove mock self-tests, a struct-literal self-check, and a false rollback test. |
| Model | 113 | Preserve safe initial execution state, explicit percentage units, storage enum strings, hierarchy validation, distribution conservation and redaction. Derived traits alone do not need tests. |
| SQLite | 198 | Preserve migrations, corrupt-field rejection, transaction atomicity, identity filtering and round trips. Many tests delegate to assertion helpers; lack of a direct assertion is not a defect. |
| Alpaca | 119 | Preserve side/quantity/fee mapping, broker identity checks and rejection before credentials or external mutations. These small adapter tests protect public translation contracts. |
| IBKR | 81 | Preserve exact quantities, contract selection, order acknowledgement, no redirects, account/environment checks and complete snapshot requirements. Local HTTP fixtures establish protocol behavior only. |
| Advisor | 24 | Preserve deterministic ATR/correlation examples, invalid thresholds, calendar HTTP responses and key redaction. |
| Service | 7 | Most service evidence already lives in integration tests. Preserve authentication, journal consistency, encrypted configuration and bounded telemetry; strengthen report outcomes. |
| Legacy CLI | 620 | Largest weak area: constructor copies, tests of scripted test input, and output-only smoke paths. Keep argument rejection, JSON contracts, protected mutations and precision. Move report correctness to actual subprocess output over persisted trades. |

Broker-sync, security and credentials use integration/property tests rather than ordinary inline tests. The review also included the broker state-machine properties and the CLI's generated lifecycle/portfolio matrices. There are no remaining `prop_assume!` calls in the Rust sources at this revision. A large generated matrix is still judged by its shared oracle, not by its number of registrations.

## Replacements

69 tests were removed, and the thin client's previous two tests were replaced with nine request/response contracts. The five product suites now provide 19 tests, often covering several boundaries and both long/short directions.

| Retired tests | Problem | Replacement or retained evidence |
| --- | --- | --- |
| 39 CLI constructors and test-double checks | Compared `.new()`/`.default()` fields or tested private scripted input defaults. | Existing command parsing/dialog outcome contracts remain. The shipping thin client now has nine real executable/HTTP contracts in [`service_client.rs`](../../cli/tests/service_client.rs). |
| 10 broker message/state checks | Derived traits, variant existence, and a conditional redaction assertion that could execute no check. | [`session_contracts.rs`](../../broker-sync/tests/session_contracts.rs) sends serialized commands through the real session tracker, verifies emitted events, duplicate replacement, selective removal and shutdown. It checks exact credential-free wire/debug output. Existing transition/property tests remain. |
| 9 report renderer smoke tests | Printed fabricated metrics without checking any output. | [`integration_test_product_reports.rs`](../../cli/tests/integration_test_product_reports.rs) checks captured text/JSON, actual fees and wins/losses, breakeven statistics, empty reports, concentration, open exposure, cash drawdown recovery and zen privacy. |
| 7 misleading report “integration” tests | Did not invoke reports; some assigned the desired status/P&L directly or tested their cleanup helper. | Real facade lifecycles populate SQLite for the CLI suite. [`product_reports.rs`](../../service/tests/product_reports.rs) records executions through authenticated MCP, checks financial results, date/account isolation, mixed-currency errors and restart/idempotent replay. |
| 4 core tests | Two tested only a mock database, one compared a literal struct with itself, and one “rollback” test failed before any debit because its source was unfunded. | Existing [`capital_contracts.rs`](../../core/tests/capital_contracts.rs) reaches an injected later-write failure, proves rollback with public reads, reopens storage and retries once. Financial concentration calculations remain tested. |

Retained argument-parsing contracts also exercise the builders’ public `Default` entry point. The independent constructor-equivalence tests are gone; parsing actual arguments now establishes that both construction and the command contract work together.

[`product_discovery.rs`](../../service/tests/product_discovery.rs) adds instrument/account discovery, exact broker resolution, currency-aware what-if and quote/tick reads, scope filtering after restart, and startup rejection before financial storage exists. These exercise successful product flows and explicit failures; preview checks prove unchanged cash/trade state and zero order submissions. The thin client also verifies operator environment configuration.

The manual-execution service contract now requires valid local reports to succeed. Only the deliberately unavailable benchmark dependency may return its expected rejection. The contract moved into a focused module, reducing the oversized parent test file without increasing its allowance.

## Defects demonstrated by stronger tests

1. The thin CLI treated a non-2xx HTTP response containing `"error": null` as success. It now exits unsuccessfully while preserving an explicit error envelope.
2. Connected broker events stripped query/password data but retained URL fragments. Fragments are now removed before serialization/logging.
3. The performance JSON consistency check omitted zero-P&L closed trades. It now accounts for breakeven trades.
4. The text report derived loss percentage as `100 - win_percentage`, incorrectly counting breakeven trades as losses. It now calculates losses divided by all closed trades.

Each defect was observed as a failing product regression before its fix. Curated mutation experiments additionally reintroduce the first two defects and replace realized losses with gains in the report calculator. A compiled mutant must fail the exact product test; a compiler failure is not accepted as evidence.

## Limits and remaining debt

- Some legacy interactive dialogs and unrelated output helpers still have print-only smoke tests. They exercise separate interactive paths and have not been relabeled as correctness tests or removed without an equivalent observable contract. Future changes to those paths must add captured output/interactive workflow assertions, not expand their smoke coverage. The production service and thin-client flows have separate acceptance tests.
- Legacy summary `equity` describes ledger cash; drawdown/timeline also subtract funding reservations before fills and include cash flows. For the new example, total ledger cash is 10,004 while the timeline carries 9,904 after a 100 long reservation (9,899 after a 105 short reservation). The tests state these distinct cash views explicitly. It is distinct from broker marked-to-market equity used by safety admission. Redefining the report's financial meaning requires a separate public-contract change.
- The session actor has no broker synchronization. Its new tests prove its actual in-memory/wire contract only.
- MCP process tests use a local gateway simulator and manually evidenced execution records. They establish Trust's accounting, transport and persistence behavior, not actual IBKR fills, fees, stop execution, OAuth deployment or live-money safety.
- Coverage remains gated at 94% lines, 90% functions and 93% regions. These thresholds supplement behavioral assertions and curated mutations; they do not certify all possible behavior.

Run instructions and test-authoring rules are in [testing.md](testing.md). Offline assurance requires the new critical contracts so an accidentally omitted test target cannot silently produce passing evidence.
