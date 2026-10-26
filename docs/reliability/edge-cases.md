# Reliability and edge-case register

Read [features](features.md) first. This is a finite, reviewable failure inventory, not a claim that software can enumerate every market outcome. Each numbered item is an obligation to check. Several inputs inside an item are equivalence classes; randomized examples do not exhaust those classes. The [verification record](verification.md) separates executed tests, new regressions, and external acceptance work. Existing tests are evidence only for the assertions they actually make.

## 1. Exact arithmetic and sizing (RISK)

1. Zero cash, no transactions, or no balance row: no positive executable quantity.
2. Negative cash or corrupt ledger: explicit failure, never a new positive risk allowance.
3. Entry zero/negative; stop zero/negative; equal entry/stop: reject execution geometry before funding.
4. Long stop above entry; short stop below entry; target on the wrong side: fail before broker I/O.
5. Tiny stop distance; large cash; large prices/quantity; Decimal multiplication, subtraction, addition or division overflow: error or conservative zero, never wrap or panic.
6. Exact per-trade risk boundary: accept a fundable quantity; one minimum quantity increment above it: reject without writes.
7. Recurring fractional quotient: round quantity toward zero to eight decimal places; never round into excess capital or risk.
8. Decimal percentages such as 0.1, 0.3, 0.7, 1.1 and 1.9: preview and funding use the same decimal interpretation of the legacy rule value.
9. Percentage zero intentionally pauses new risk. Negative, NaN, infinity, over 100, or unrepresentable conversion: rejected at rule creation/restore; persisted corruption fails closed at use.
10. No risk rules versus only monthly versus only per-trade versus both: document actual policy; do not silently imply a cap exists when it does not.
11. Rule reads or balance reads fail: propagate failure; an unavailable risk rule is not an empty rule set.
12. Reordered rule priorities, duplicates, inactive or deleted rules: active effective rules must govern consistently.
13. Long funding uses entry notional; short funding uses stop notional; each remains bounded by available cash.
14. Fractionable instruments permit supported fractional execution; non-fractionable stocks/ETFs/bonds reject it before submission.
15. Level zero through four, risk multipliers below/at/above one, zero/negative/overflowed multiplier: enforced quantity and preview remain consistent with the base risk and capital ceilings.
16. High-precision decimal input and JSON numeric input: exact representable strings only, never silently round a monetary request.

Evidence: `core/tests/risk_invariants_test.rs`, `core/tests/reliability.rs`, quantity/funding validators, model rule validation, IBKR HTTP integration tests, service catalogue contract tests.

## 2. Monthly and portfolio risk (MONTH)

1. Month with no opening capital, first deposit during the month, or no free cash: explicit conservative result.
2. Beginning-of-month boundary in UTC, previous month/year, leap day: transactions enter exactly one window.
3. Realized profits, realized losses, all risk allowance consumed, and loss beyond the allowance: correct residual budget and no extra risk after exhaustion.
4. Funded long position consumes planned stop loss, not its entire notional.
5. Funded short position consumes planned stop loss even though stop collateral is reserved.
6. Submitted, partially filled and filled positions remain in the open-risk calculation.
7. New and terminal zero-exposure trades do not manufacture protected capital.
8. Multiple simultaneous longs, shorts and mixed sides: the sum of planned losses cannot disappear when status changes.
9. Partial entry followed by cancel/expire/reject: remaining exposure remains protected and counted.
10. Partial exit, two exits, or a residual position: use remaining exposure and retained cash, not original full quantity as if nothing had exited.
11. Better entry and stop tightening: count only persisted prices/executions and remaining quantity.
12. Fees, adverse stop slippage and gaps reduce realized cash; planned loss remains an estimate, never a guaranteed loss cap.
13. USD, EUR and BTC balances and unrelated accounts: do not cross-subsidize or sum without conversion.
14. Deposit/withdrawal/transfer effects around month boundaries: follow the ledger policy rather than infer investment performance from unrelated records.
15. Open-position query failure or malformed row: reject risk calculation rather than return an empty portfolio.

Evidence: monthly/available-capital calculators, `core/tests/reliability.rs`, SQLite transaction boundary tests, risk/report integration suites. Live gap execution is an external acceptance item.

## 3. Accounts and capital (CASH)

1. First deposit creates the right currency projection once.
2. Zero/negative deposit or withdrawal: reject with no ledger/projection mutation.
3. Fractional cents and very large but representable balances: preserve exact decimals.
4. Withdraw exactly available cash versus one increment more: accept boundary, reject excess.
5. Capital locked in a funded/open position cannot be withdrawn or transferred.
6. Deposit overflow and projection update failure after transaction insertion: roll back the complete operation.
7. Transfer has two equal opposite legs in the same currency and one auditable reason.
8. Transfer fails after its first leg or destination projection: no unilateral debit.
9. Transfer to self, unrelated hierarchy, deleted account, missing account, or wrong currency: reject as specified.
10. Account create fails while initializing level/balances: no orphan account.
11. Account deletion with open trades, balance or children: preserve blocking checks; force cannot remove active exposure.
12. Duplicate/blank names, missing broker account binding, unsupported broker, invalid environment and invalid percentages: validate profile boundaries.
13. Stale caller account data must not change persisted broker, environment or owner identity.
14. Two concurrent valid withdrawals whose sum exceeds cash: serialized result cannot overspend.
15. Restart after accepted deposit/transfer: replay returns original outcome and ledger count remains unchanged.

Evidence: transaction integration suites, account/hierarchy tests, distribution atomicity tests, security regressions and service reliability tests.

## 4. Trade creation and lifecycle (STATE)

1. Draft creation creates all three orders and trade or creates nothing.
2. Non-positive quantity/price, unsupported safety-order type, bad geometry or unsupported asset: reject without orphan orders.
3. Fund only persisted New state; repeated/stale funding cannot reserve twice.
4. Submit only persisted Funded state; two connections cannot submit twice.
5. Filled and partially filled states cannot be replaced by a delayed submission acknowledgment.
6. Manual fill only from allowed persisted states; duplicate fill cannot book twice.
7. Manual negative fee or invalid actual fill price: reject atomically.
8. Cancel funded trade returns exact reserved cash once; stale retry fails.
9. Submitted cancellation with no confirmed broker response retains uncertainty and collateral.
10. Cancellation after partial entry must retain exposure until it is actually closed.
11. Close request is not a confirmed close; keep cash reserved until reconciliation confirms exits.
12. Double close, stale close and late close acknowledgment cannot overwrite an existing exit.
13. Filled long stop cannot move down; filled short stop cannot move up.
14. Stop/target zero, negative, wrong geometry or wrong lifecycle state: no broker mutation.
15. Concurrent stop/target modifications and an unknown previous outcome: at most one unresolved attempt per order.
16. Reconcile the observed price before allowing another uncertain modification.
17. Modified broker order IDs replace the proper role without losing execution identity.
18. Explicit manual close reason, broker target and stop provenance remain distinguishable.
19. Every combination of lifecycle operation and persisted state must either follow the allowed transition with accounting or fail without a side effect.
20. A caller-supplied foreign account ID or stale balance/status must never select another broker or bypass the persisted state.

Evidence: `core/tests/security_regressions.rs`, trade validators, CLI stale-state/lifecycle suites, broker HTTP tests and service submission tests.

## 5. Reconciliation, executions and fees (SYNC)

1. Identical cumulative broker snapshot repeated: zero additional financial effect.
2. Intermediate snapshots skipped: book the cumulative delta exactly once.
3. Decreasing cumulative filled quantity: explicit rejection and rollback.
4. Fill quantity below zero or above order quantity; exits above entries: reject the payload.
5. Partial fill followed by terminal entry status: retain residual exposure.
6. Two exit orders both partially filled: sum them; do not assume only one executed.
7. Broker says closed with residual exposure: reject inconsistent closure.
8. Filled order without average price, non-positive price or inconsistent role/quantity: reject safely.
9. Legacy filled snapshot with zero cumulative quantity: documented full-fill normalization only for Filled.
10. Wrong broker account/order/trade identity: no financial update.
11. Duplicate execution ID, same ID in another account, conflicting replay: enforce identity scope and conflict checks.
12. Fee received before fill, after fill, after close, repeatedly or in a later batch: book once by activity identity.
13. Opening/closing fee classification and currency mismatch: preserve correct ledger category/currency.
14. Negative fees/rebates, unknown activity types or unparseable amounts: explicit supported semantics or rejection; do not guess.
15. Partial order update, execution insert or fee write fails: rollback orders, status, executions, fees and balances together.
16. Missing bracket leg, stale embedded leg, authoritative direct order lookup and reordered broker arrays: resolve by identity, not array position.
17. Lost broker response after acceptance: operation stays uncertain; no automatic second external mutation.
18. Transport retry/redirect must not repeat or forward a mutating request.
19. Polling without status change may still contain new executions or fees.
20. Reconciliation across multiple trades is bounded; errors and continuation cursor are visible.

Evidence: reconciliation/security regressions, execution/fee workers, Alpaca sync mapper tests, IBKR HTTP/security tests, service idempotency tests.

## 6. Atomicity and crash boundaries (ATOMIC)

1. Begin savepoint fails: no mutation begins.
2. Fail each write of create/fund/fill/cancel/close/deposit/withdraw/transfer/distribution: no partial ledger state.
3. Release/commit fails: attempt rollback and surface commit uncertainty.
4. Rollback or release-after-rollback fails: do not report a clean rollback.
5. Nested savepoints and invalid savepoint names: isolate valid inner work and reject unsafe names.
6. Intent journal unavailable: do not invoke the domain or broker.
7. Domain committed, result journal fails: stop worker and expose unknown outcome; never rerun automatically.
8. Crash after durable intent but before dispatch: mark outcome unknown on restart.
9. Crash during broker I/O, after broker acceptance or before durable acknowledgment: reconcile using stable references.
10. Crash after durable result but before client receives it: original key recovers the result.
11. Client disconnects while queued: do not execute a request that has not started; already started work keeps durable outcome.
12. Graceful shutdown waits for active financial work and rejects new work.
13. A second process opens the same financial/journal files, including aliased paths: reject competing worker ownership.
14. Corrupted journal hash, missing event, forged payload or interrupted journal transaction: detect before serving writes.
15. Disk full, I/O failure, power loss and filesystem corruption: use fail-stop behavior; real hardware durability is not established by an in-memory test.

Evidence: savepoint/fault-injection suites, `core/tests/reliability.rs`, journal/service contract tests; host power-loss/storage failures require separate deployment drills.

## 7. Service idempotency and load (SERVICE)

1. Every mutating tool requires an idempotency key; reads must not accidentally acquire financial side effects.
2. Same subject/key/tool/arguments: return original result across concurrent calls and restart.
3. Same key with changed amount, account, tool or arguments: conflict, no second mutation.
4. Same key used by different subjects: intentionally separate namespaces, each still authorized.
5. JSON object key reordering: canonical hashing preserves logical replay.
6. Malformed arguments never reserve a key or reach broker I/O.
7. Failed mutation replay remains failed/uncertain and cannot execute again under the same key.
8. operation_get unknown key, another subject's key, revoked account or removed secondary account: no result disclosure.
9. Queue full, worker stopped, readiness false: actionable rejection and no hidden new intent.
10. Request timeout while executing: unknown outcome and inspection instructions, not a false rollback claim.
11. Oversized response after successful mutation: preserve durable operation identity and prevent replay.
12. Concurrent mutations cannot bypass financial invariants even with different valid keys.
13. Periodic reconciliation failure, disabled interval and restart cursor: bounded, visible behavior without duplicate booking.
14. Pagination empty/exact-page/last-page/large offset; invalid limits and cursors: bounded consistent output.
15. Rate limits per process and subject; independent agents cannot bypass the global ceiling.

Evidence: service contract/reliability tests and hardened-container smoke test. Saturation throughput and long soak tests are distinct from functional concurrency tests.

## 8. Authentication, authorization and secrecy (AUTH)

1. Missing/malformed bearer token, unknown signing key, wrong signature/algorithm: 401 before dispatch.
2. Wrong issuer or resource audience, expired token, missing lifetime, future iat/nbf or excessive lifetime: deny.
3. Unknown subject, empty grant, invalid scope or explicitly revoked jti: deny.
4. Token scopes are intersected with operator grants, never unioned.
5. All 82 tools require their declared scope; lower-privilege discovery omits unavailable tools.
6. Every affected account is authorized, including transfer destination, comparison account and distribution children.
7. Live access requires both global enablement and subject live permission; paper authorization cannot imply live authorization.
8. A trade/rule/session ID from another account cannot bypass account-scoped permission.
9. Replaying a stored result rechecks every originally affected account.
10. Global instrument administration and backups cannot be delegated accidentally by a narrow account grant.
11. Host and Origin mismatch, wildcard/empty Origin tricks, credential-bearing query and invalid Content-Type: enforce request boundary.
12. Bodies, arrays, strings, numbers and nesting exceeding supported limits: reject without worker effect.
13. Errors/debug/logs/API responses never include tokens, keychain secrets, provider keys, hashes or credential-bearing URLs.
14. JWKS/grant/revocation changes take effect on restart; rotation is operationally explicit.
15. Database/journal/private configuration permissions and read-only non-root container constraints survive restart.

Evidence: JWT/HTTP service tests, tool-scope matrix, adapter redaction/account tests, model serialization checks and container validation.

## 9. Tool schemas and agent UX (TOOLS)

1. Every catalogue entry is unique, has a real operation and strict input schema.
2. Every required field omitted individually: invalid_arguments before domain access.
3. Unknown fields and null/wrong types for every property: invalid_arguments.
4. Decimal money strings versus integers/floats, NaN/Infinity, scientific notation, overflow and excess precision: reject unsupported representations.
5. UUID malformed/foreign/missing; enum unknown/case mismatch; string empty/NUL/too long: explicit rejection.
6. Integer min/max and one outside; bool versus string; array empty/too long/wrong element: enforce schema.
7. All tools can be discovered through authenticated HTTP and MCP with equivalent schemas and scopes.
8. Skills list/read and MCP resources/prompts expose all three installed workflows and reject unknown names.
9. Successful prepare/fund/submit/modify returns the next safe action; uncertain result directs operation inspection/reconciliation.
10. A CLI failure or stdout formatting problem cannot turn an unsuccessful request into success.
11. Plaintext errors and JSON errors remain stable and omit raw internal credentials.
12. Read reports state that broker previews, local balances, planned risk and actual execution are different observations.

Evidence: complete catalogue schema/scope matrix, real HTTP/MCP contract, CLI service-client tests and legacy JSON snapshots.

## 10. Leveling, grades and profit distribution (POLICY)

1. Default level initialization; duplicate initialization; unsupported target level; invalid policy key/value.
2. Threshold exactly met versus just below/above for wins, losses, monthly loss, drawdown, sample size and cooldown.
3. Forced/manual level change requires reason/authority and preserves history; exact retry is idempotent.
4. Level cache refresh after close/configuration/restart; no stale multiplier for funding.
5. Grade empty/unclosed trade, invalid geometry, missing timestamps/equity and zero risk: explicit grading behavior.
6. All component weights, saturated/overflowing weights and bounded scores: deterministic totals.
7. Explicit regrade appends a snapshot; repeated reconciliation does not create duplicate automatic consequences.
8. Distribution allocation percentages sum to 100; reject negative, over-100 and invalid sums.
9. Zero/loss/below-threshold profit: no transfers; threshold equality and positive profit: documented allocation.
10. Rounding/remainder across earnings/tax/reinvestment/insurance: no creation or loss of money.
11. Missing/wrong child types, parent mismatch, currency mismatch and insufficient available profit: reject atomically.
12. Failure on any allocation leg or history insert: roll back all transfers and history.
13. Duplicate close/reconcile: no second distribution for the same financial close.
14. Configuration secret missing, weak password or malformed stored hash: no unauthorized configuration; never return the hash.
15. Closing succeeded but post-close grading/level/distribution fails: retain truthful close state and surface follow-up failure.

Evidence: model distribution/level tests, service/profit distribution suites, database atomicity tests, grade/level CLI integration and security regressions.

## 11. Reports, analytics and research (ANALYSIS)

1. Empty portfolio/history, one sample, zero variance and all wins/all losses: deterministic finite result or explicit insufficient-data error.
2. Mixed currencies/accounts: reject invalid aggregation and enforce comparison-account permission.
3. Reversed/equal/extreme time range, timezone offsets, UTC date boundaries: validated window with correct baseline.
4. Deposits/withdrawals versus trading return: do not classify cash flows as realized trade profit.
5. Long versus short signed P/L, fees, slippage and partial exits: metrics agree with ledger.
6. Drawdown peak, trough, recovery, never-recovered series and date overflow: correct extrema and explicit limits.
7. Decimal square root/log, division by zero, skew/kurtosis/tail estimates and bootstrap insufficient inputs: no panic/NaN.
8. Concentration thresholds zero/100/out of range, unknown sector/asset class and duplicate symbols: deterministic advisory result.
9. Correlation constant/short/misaligned price series, invalid threshold, missing bars and transitive clusters: explicit evidence and insufficient-data handling.
10. Regime with insufficient history, invalid thresholds, flat prices and elevated/choppy market: documented signal, no false enforced guarantee.
11. Catalyst provider unavailable/missing key/malformed event/time window: preserve warnings and avoid fabricating events.
12. Bond zero/negative face/price, coupon zero, maturity zero/negative, frequency invalid, accrued schedule boundary: Decimal-only and validated.
13. Tax lots, wash-sale window edges, replacement quantities and partial matching: deterministic application policy; jurisdiction completeness is not asserted.
14. Attribution, benchmark and timeline with missing benchmark or zero baseline: explicit unavailability rather than guessed numbers.
15. Session already open/closed, no active session, invalid grade/regime/setups and max-position bounds: preserve auditable plan/review state.
16. Event severity/type/date and mistake tendencies/counterfactual values: validate domain and account linkage.

Evidence: calculator/advisor suites; report/metrics/grade/session integration tests; service report handlers and source-level coverage index. Real market-data fidelity remains broker acceptance work.

## 12. Brokers, storage, packaging and operations (OPS)

1. IBKR paper/live evidence absent/contradictory; wrong selected account: fail before order submission.
2. Contract resolution ambiguous/empty/unsupported security type; bond mapping versus ETF/stock: typed and deterministic.
3. Broker preview commission/margin absent, range-valued, malformed or lacking currency: return unavailable fields, never infer an amount/currency.
4. HTTP 401/403/429/5xx, malformed JSON, disconnect, timeout and redirect: bounded errors; no automatic mutation replay.
5. Existing full/partial bracket IDs and broker confirmation flow: do not duplicate accepted legs or automatically confirm an ambiguous warning.
6. Alpaca credential file wrong account/environment/endpoint or malformed secret: fail closed; local credential store unavailable is not an in-memory fallback.
7. Historical bars/quote/trade missing, negative volumes, unsupported stream channels and finite-event timeout: preserve normalized contract and source evidence.
8. SQLite FK enforcement, migrations up/down, old schema, malformed UUID/Decimal/status and soft-deleted rows: reject corruption and preserve references.
9. Backup every application table including empty/soft-deleted rows; validate version/migration manifest, domain values and FK graph.
10. Restore into nonempty database, wrong artifact ID/path traversal, corrupt JSON/oversize/missing tables: reject without partial replacement.
11. Failed export must preserve previous good backup; failed restore must preserve destination state.
12. Audit alteration/removal/reorder/fork, legacy hash compatibility and operation-argument canonicalization: detect or preserve as specified.
13. Non-root container, read-only filesystem, private writable state, unavailable secrets and restart: functional readiness/health and no leaked tokens.
14. Dependency advisories, unsafe workflow interpolation, inconsistent locks/toolchain and new tool without test coverage: CI must fail.
15. Broker-sync actor disconnect/backoff/jitter/message ordering is tested as a scaffold; production websocket recovery is not implemented.
16. Real exchange halt, overnight gap, split/corporate action, margin recall, borrowing change, fractional routing/tick-size rejection and upstream outage: require recorded broker paper/live-safe evidence before claiming support.
17. Real disk exhaustion, sudden host power loss, disaster restore from independently stored backups and multi-day soak: require deployment drills with retention/observability evidence.
18. OAuth provider key rotation/revocation propagation and network TLS/proxy configuration: verify in the actual deployment; local JWT tests are only component evidence.

Evidence: adapter suites, SQLite backup/migration/worker tests, workflow checks and hardened-container smoke. Items 16–18 are explicit external acceptance gates; no live-money orders are authorized by this test task.
