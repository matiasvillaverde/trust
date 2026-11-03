# Test usefulness catalog — 2026-10-31

This catalog lists **every one of the 2,500 registered Rust tests**, gives each a usefulness estimate, and identifies what to review or replace. It also lists **46 Python tests, 15 shell checks, six orphan Rust tests, and the acceptance/evaluation cases** outside ordinary test discovery. The complete [sortable CSV](test-usefulness/catalog.csv) contains 2,637 entries. Those entries are **not 2,637 independent automated tests**: acceptance checkpoints, shared agent scenarios and mutation experiments are explicitly distinguished.

The clearest remaining debt is in the legacy CLI, but there are financial examples to strengthen too. A test that only checks `Some`, accepts either success or failure, or swallows a panic can pass while its advertised behavior is broken. Conversely, a small Decimal calculation, parser, redaction or transaction test can be more valuable than a large integration test.

Baseline: [`3e8718f31bf0`](https://github.com/matiasvillaverde/trust/commit/3e8718f31bf0627e51c26123c06e4f0fc6224abe), after PR #156. This is a **snapshot audit**, not a new test-execution report or a certification of trading safety. No production code or tests were changed for this catalog.

**Follow-up:** the [2026-11-01 replacements](test-usefulness-improvements.md) address all 112 low-value candidates (including the 52 unchecked output/routing cases) and the six orphan functions. The scores and source links below remain pinned to the original audit; they are not ratings of the replacements.

## What the probabilities mean

The percentage is a **subjective estimate that the current test meaningfully protects a product, security, persistence or operational contract and is worth retaining substantially as written**. It is not the probability of catching an arbitrary bug, preventing account loss, or passing CI. The ranges are deliberately coarse; they are not statistically calibrated. Do not average them into a safety score.

| Estimated usefulness | Range | Registered Rust cases | Interpretation |
| --- | --- | ---: | --- |
| Strong | 81–95% | 135 | Concrete behavioral/fault evidence, often across persistence or a public boundary. Preserve it. |
| Useful | 61–80% | 2,026 | An observable result is checked; independence and fixture quality still matter. Most indexed estimates are here. |
| Mixed | 41–60% | 227 | Some useful protection, with a weak result check, coupled expectation, broad rejection or repetition. Strengthen it. |
| Limited | 21–40% | 47 | Mostly internal wiring, crash behavior, permissive branching or a weaker oracle than the name promises. |
| Minimal | 1–20% | 65 | Unchecked rendering, discarded outcomes, test-double self-tests or incidental derived formatting. |
| Inactive | 0% **current coverage** | 6 additional source functions | Not registered by Cargo. This measures present coverage contribution, not the potential value of a future replacement. |

These are triage estimates, **not a deletion list**. A low-scoring financial test may be more urgent to improve than dozens of low-scoring presentation tests. An ignored test is scored for what it checks **when deliberately run**; it contributes no evidence to a default run that skips it.

## How much review each estimate has

Every registered test was mapped to its source, assertions, calls and resolvable local test helpers. Macro-generated cases retain their individual names and shared oracle. Body review focused on weak candidates, financial exceptions and acceptance workflows; the prior product-contract review and mutation evidence were retained where applicable.

| Review basis for Rust cases | Count | Meaning |
| --- | ---: | --- |
| Indexed | 1,955 | Automated source-assisted triage. **Low confidence**, pending individual setup/oracle review. |
| Family review | 447 | Shared patterns/helpers and representative contracts inspected. Does not claim a separate proof for each case. |
| Body review | 51 | Specific body and relevant helper behavior inspected for the stated finding. |
| Prior contract review | 21 | Detailed behavioral review from the preceding cleanup, retained at this source revision. |
| Mutation evidence | 19 | The linked test caught one specific compiled production fault in the preceding validation. |
| Lane review | 7 | Required environment/opt-in and scope inspected; execution is separate. |

Confidence is separate from usefulness: **119 high, 426 medium and 1,955 low** among Rust cases. A high-confidence low score means the weakness is clear. A low-confidence “Useful” score is provisional, not an endorsement that its setup or expected result is correct. This is a complete inventory with targeted detailed review, **not a claim that all 2,500 bodies received an exhaustive manual review**.

Scores consider the production entry point, observed result, valid preconditions, independence of the expectation, fault reachability, relevant side effects, and brittleness/cost. Names, assertion counts, line coverage and placement in `tests/` are insufficient. Generic error/success assertions are review signals; shared assertion helpers, expected SQL success and property-runner errors can be valid oracles without a direct `assert!`.

## Complete catalog

Each row has a source link, estimate, confidence, review basis, concrete assertion/check excerpt, limitation and action. The CSV adds full Cargo identities, execution status, helper provenance, shared-case families and body hashes. Excerpts are representative, sometimes shortened; the linked source is authoritative.

| Area | Cases | Catalog |
| --- | ---: | --- |
| Core calculations | 249 | [Financial formulas and boundaries](test-usefulness/core-calculators.md) |
| Core orchestration | 237 | [Commands, validators, services and facade](test-usefulness/core-workflows.md) |
| Core contract suites | 102 | [Capital, risk, security, history and reliability](test-usefulness/core-contracts.md) |
| Service | 84 | [HTTP/MCP, authority, journal, process E2E, encryption and skills](test-usefulness/service.md) |
| Domain model | 113 | [Model contracts](test-usefulness/model.md) |
| SQLite | 209 | [Persistence, migration, backup and rollback](test-usefulness/db-sqlite.md) |
| Security | 4 | [Private files and key custody](test-usefulness/security.md) |
| IBKR | 115 | [Adapter, local HTTP and ignored gateway tests](test-usefulness/ibkr-broker.md) |
| Alpaca | 119 | [Adapter contracts](test-usefulness/alpaca-broker.md) |
| Broker session tracking | 38 | [Actor, serialization, recovery state and properties](test-usefulness/broker-sync.md) |
| Advisor | 24 | [Catalysts, correlation, configuration and regime](test-usefulness/advisor.md) |
| CLI commands/dialogs | 342 | [Parsing, interactive paths and utilities](test-usefulness/cli-commands-dialogs.md) |
| CLI dispatcher | 180 | [Routing and handler contracts](test-usefulness/cli-dispatcher.md) |
| CLI views | 44 | [Presentation checks](test-usefulness/cli-views.md) |
| CLI integration suites | 240 | [Executable/client and legacy feature suites](test-usefulness/cli-integration.md) |
| Legacy bug contracts | 188 | [Individual exploratory/regression cases](test-usefulness/cli-bug-contracts.md) |
| CLI lifecycle scenarios | 212 | [Named scenarios, generated matrices and performance gate](test-usefulness/cli-lifecycle-matrices.md) |
| Orphan source | 6 | [Functions Cargo does not register](test-usefulness/inactive.md) |
| Python and shell | 61 | [46 unittest cases and 15 workflow checks](test-usefulness/automation.md) |
| Acceptance | 18 | [OIDC checkpoints/subchecks, container and paper workflows](test-usefulness/acceptance.md) |
| Agent evaluations | 33 | [Nine shared-fixture scenarios and 24 routing cases](test-usefulness/agent-evaluations.md) |
| Mutation experiments | 19 | [Specific faults and their detecting tests](test-usefulness/mutations.md) |

The credentials crate has **zero directly registered tests**. Other crates exercise some credential interfaces, so this is a direct-test gap, not proof that every credential path is untested. Native backend selection, permission failures and platform-specific behavior need explicit coverage mapping before adding tests.

## Findings that should drive the next changes

| Priority | Finding and evidence | Useful replacement or improvement |
| --- | --- | --- |
| High: financial oracle | [`test_calculate_expected_shortfall_returns_tail_average`](https://github.com/matiasvillaverde/trust/blob/3e8718f31bf0627e51c26123c06e4f0fc6224abe/core/src/calculators_advanced_metrics.rs#L2390) checks only `es.is_some()`. An incorrect tail average passes. | Assert independently worked Decimal tail averages, including repeated losses, confidence boundaries and empty/invalid inputs. Keep this a focused calculation test. |
| High: misleading failure scenario | [`test_broker_cancel_fails_trade_stays_funded`](https://github.com/matiasvillaverde/trust/blob/3e8718f31bf0627e51c26123c06e4f0fc6224abe/cli/tests/integration_test_broker_failures.rs#L1064) never submits the trade and accepts either cancellation success or failure. | Separate cancellation before submission from failure after submission; prove the intended broker boundary is reached and verify reservation, status, receipts and call count. |
| High: false green | [`test_dispatch_trade_interactive_variants_are_reachable`](https://github.com/matiasvillaverde/trust/blob/3e8718f31bf0627e51c26123c06e4f0fc6224abe/cli/src/dispatcher/tests/trade_commands_3.rs#L12) catches panics from nine routes and discards the result. | Exercise meaningful command flows with controlled input, assert the result and resulting state, and fail on unexpected panics. |
| High: permissive rejection | [Invalid draft cases](https://github.com/matiasvillaverde/trust/blob/3e8718f31bf0627e51c26123c06e4f0fc6224abe/cli/tests/integration_test_bug_hunt.rs#L488) allow any creation error; the unchanged-cash baseline is taken afterward. [Other zero-risk/quantity tests](https://github.com/matiasvillaverde/trust/blob/3e8718f31bf0627e51c26123c06e4f0fc6224abe/cli/tests/integration_test_sync_edge_cases.rs#L841) can skip their funding assertion entirely. | Snapshot state before the attempt, establish valid unrelated preconditions, assert the intended rejection boundary and add a valid neighboring request. |
| High: skipped credential contract | [`connection_helpers_roundtrip_through_public_api`](https://github.com/matiasvillaverde/trust/blob/3e8718f31bf0627e51c26123c06e4f0fc6224abe/ibkr-broker/src/lib.rs#L984) performs its assertions only if credential setup succeeds. | Use an explicit deterministic store for the ordinary contract; give actual native-backend acceptance a configured lane with a visible skip/failure outcome. |
| Medium: unchecked output | **52 CLI tests** call rendering/routing paths without checking output or resulting state. For example, [`display_functions_run_for_smoke_coverage`](https://github.com/matiasvillaverde/trust/blob/3e8718f31bf0627e51c26123c06e4f0fc6224abe/cli/src/views/account_view.rs#L144). | Capture text/JSON or returned data from real feature scenarios. Check amounts, identifiers, empty/error cases and secret/zen handling; reuse existing product suites where they cover the same contract. |
| Medium: testing the double | Six `*_aux_methods_have_expected_contracts` tests call test-only broker implementations directly, for example [`ModifyOkBroker`](https://github.com/matiasvillaverde/trust/blob/3e8718f31bf0627e51c26123c06e4f0fc6224abe/cli/src/dialogs/modify_dialog.rs#L690). | Use the double to drive a facade/service workflow and assert a production outcome. Remove duplicate self-checks once unique harness requirements are accounted for. |
| Medium: accepting `Some(Err(...))` | Several dialog tests assert only `Option<Result<...>>::is_some()`, for example [`build_stop_and_target_call_facade_when_required_fields_are_present`](https://github.com/matiasvillaverde/trust/blob/3e8718f31bf0627e51c26123c06e4f0fc6224abe/cli/src/dialogs/modify_dialog.rs#L508). | Require the intended success or specific rejection and read back trade/protection state; an arbitrary failure must not satisfy a success-path test. |
| Medium: dead coverage | Six tests in [`cli/src/output.rs`](https://github.com/matiasvillaverde/trust/blob/3e8718f31bf0627e51c26123c06e4f0fc6224abe/cli/src/output.rs#L263) are outside the active module tree. `dispatcher::output` resolves to a different file. | Port any unique contract to the active renderer, then retire the orphan. Registration reconciliation must stay part of future audits. |
| Medium: duplicated expectation | [`create_sets_default_settlement_date_by_asset_category`](https://github.com/matiasvillaverde/trust/blob/3e8718f31bf0627e51c26123c06e4f0fc6224abe/db-sqlite/src/workers/worker_trade.rs#L685) derives the expected value from the production settlement helper. | Retain wiring coverage, but use fixed calendar examples to independently establish persisted settlement behavior. |
| Medium: costly weak matrix oracle | **120 generated lifecycle cases and 29 generated portfolio cases** share two helpers. The portfolio helper checks counts/ranges and replay of the same ledger, without independently calculating final P&L. | Add exact independent cash/P&L/fee expectations, then select cases by meaningful dimensions and keep a separately justified stress workload. Do not remove unique long/short, fill, exit or replay cases based on count alone. |
| Lower: incidental representation | Six SQLite tests pin derived/internal Debug strings, for example [`debug_representation_is_stable`](https://github.com/matiasvillaverde/trust/blob/3e8718f31bf0627e51c26123c06e4f0fc6224abe/db-sqlite/src/workers/worker_trade.rs#L1056). Other placeholder-debug tests have a plausible redaction goal but no realistic secret marker. | Consolidate incidental formatting checks; retain explicit wire formats and secret-exclusion contracts. |
| Lower: source-text “integration” | [Workflow checks](https://github.com/matiasvillaverde/trust/blob/3e8718f31bf0627e51c26123c06e4f0fc6224abe/.github/test_workflow_integration.sh#L74) look for `gh release create`, `matrix:` or one checkout SHA. Text in a comment can satisfy several checks. | Parse job/event/permission structure, check every action pin, and rely on actual built-artifact/install behavior for release acceptance. |

Additional low-scoring cases and exact recommendations are in the catalog; the table is a prioritized sample, not the complete list. Not every source check is bad: the [thin-client dependency test](https://github.com/matiasvillaverde/trust/blob/3e8718f31bf0627e51c26123c06e4f0fc6224abe/cli/tests/architecture_guard.rs#L7) inspects the actual Cargo graph, which is an appropriate packaging contract.

## What should stay small and fast

- Preserve exact Decimal formulas, percentage-unit boundaries, stop-direction invariants and parser rejection. Strengthen independent expected values rather than adding process setup to pure calculations.
- Preserve real SQLite rollback, restart, corruption, isolation and bounded-history contracts. A storage test legitimately checks persisted values even when its inputs are literal fixtures.
- Preserve the [capital contracts](https://github.com/matiasvillaverde/trust/blob/3e8718f31bf0627e51c26123c06e4f0fc6224abe/core/tests/capital_contracts.rs), [shipping CLI HTTP contracts](https://github.com/matiasvillaverde/trust/blob/3e8718f31bf0627e51c26123c06e4f0fc6224abe/cli/tests/service_client.rs), and [service report/discovery contracts](https://github.com/matiasvillaverde/trust/blob/3e8718f31bf0627e51c26123c06e4f0fc6224abe/service/tests/product_reports.rs). They check observable effects and have previously exposed real defects.
- Preserve positive security cases alongside denials. For example, [`production_accepts_ram_mounted_secrets`](https://github.com/matiasvillaverde/trust/blob/3e8718f31bf0627e51c26123c06e4f0fc6224abe/security/tests/private_storage.rs#L94) has no `assert!` but meaningfully requires an actual private RAM-mounted key to be accepted. [Migration success](https://github.com/matiasvillaverde/trust/blob/3e8718f31bf0627e51c26123c06e4f0fc6224abe/db-sqlite/src/migration_fk_safety_tests.rs#L195) and [property-runner failures](https://github.com/matiasvillaverde/trust/blob/3e8718f31bf0627e51c26123c06e4f0fc6224abe/core/tests/account_safety_test.rs#L325) are similar counterexamples to assertion-count scoring.

For replacements, follow [test contracts](testing.md): assert the result and relevant side effects, reach the intended fault, pair invalid boundaries with valid ones, and demonstrate failure under a plausible regression. Keep an old test until any unique behavior has a concrete replacement; one strong feature scenario can replace multiple implementation-coupled probes.

## Actions

| Code | Meaning |
| --- | --- |
| KEEP | Preserve the existing useful boundary and independent example/invariant. |
| READ | Individually inspect setup and oracle independence before accepting the provisional indexed rating. |
| OUTPUT | Capture text/JSON/typed results or resulting state from a meaningful feature flow. |
| PANIC | Assert safe user-visible failure and unchanged state; do not require or swallow a crash. |
| ERROR | Require the intended rejection, valid neighboring inputs and relevant unchanged side effects. |
| SUCCESS | Check returned values/persistence, so a silent no-op cannot pass. |
| CONSOLIDATE | Reduce incidental or overlapping checks while preserving public formats and real redaction behavior. |
| MATRIX | Improve independent financial expectations and justify repeated-case runtime. |
| SOURCE | Replace approximate text matching with parsed contracts or executable evidence. |
| EXTERNAL | Run only in the configured acceptance lane and retain its evidence. |
| AGENT | Execute an external agent and independently grade both report and tool trace. |
| ORPHAN | Port unique behavior to a registered target/module, then remove dead source. |
| INDEPENDENT | Calculate the expectation separately from the production function under test. |
| REDACT | Check that sensitive marker values are absent, rather than merely pinning an internal placeholder. |

## Inventory and verification limits

A fresh registration completed successfully with the pinned Rust 1.99.1 toolchain:

```bash
cargo test --locked --workspace --all-features -- --list
```

It lists 2,500 Rust cases: 2,342 ordinary functions, nine `proptest!` declarations and 149 individually registered macro cases. Nine cases are ignored by default. Source mapping has **zero unresolved registrations**. The six extra orphan functions are reported separately. The inventory is for this Linux/all-features configuration; it is not evidence for every feature combination or operating system.

Python AST discovery found 46 unittest methods across `scripts/`, `.github/tests/` and `tests/`. Shell duplicate `if/else` calls count once per named check. Doctests register no cases here. Helper functions, fixtures, snapshot files, build/lint jobs and wrapper scripts are not separate tests. The source-blind agent fixture counts once among Rust tests; its nine scenarios are configuration entries. The Python MCP probe and OIDC checkpoints share one sequential acceptance run. Mutation experiments reference existing tests rather than adding 19 more ordinary test cases.

This documentation pass **listed and inspected tests; it did not rerun the functional suite, performance gates, container/browser workflows, agent trials or live brokers**. Historical validation of the identical code tree recorded 2,491 passed Rust tests, zero failures and nine ignored, with 19/19 curated compiled faults caught. Those results belong to [PR #156](https://github.com/matiasvillaverde/trust/pull/156) and its [CI run](https://github.com/matiasvillaverde/trust/actions/runs/41215645711); they are not new evidence produced by this audit. The [inventory manifest](test-usefulness/inventory.json) records the baseline, counts and artifact hashes.

Local gateway simulations do not establish real broker fills, stop execution, fees or reconnect behavior. Metadata-only skill routing does not establish actual agent behavior. Virtual-authenticator OIDC checks do not establish a physical YubiKey or production deployment. Use the [verification levels](verification.md), [paper evidence workflow](../ibkr-paper-evidence.md) and [agent evaluation workflow](agent-evaluations.md) for those claims.

To refresh this snapshot, list the configured Cargo targets again, reconcile every registered identity against source (including macros and helper oracles), update the per-case CSV and confidence/basis, regenerate the linked tables, and validate counts, source links and hashes. Reassess ratings when setup or assertions change; a body hash alone does not capture changes to helpers, production code or external systems. Promote an indexed rating only after inspecting its preconditions and independent outcome, or demonstrating a meaningful fault that it catches.
