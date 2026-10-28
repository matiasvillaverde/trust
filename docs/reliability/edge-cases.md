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
16. Winning/recovery criteria and either loss threshold are simultaneously met, including exact equality: a loss-driven downgrade wins at every level/status; the floor cannot upgrade and progress cannot advertise a winning path.
17. Minimum-trade or recent-change stabilization limits reached: loss-driven downgrades still apply and append audit history; performance upgrades remain constrained. Newly funded and previously reserved long/short entries must observe the reduced level before broker dispatch.

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


## Service operation evidence and failure recovery

1. An authenticated HTTP mutation and its concurrent MCP duplicate share one operation ID and cause one broker submission; both request IDs remain traceable.
2. Restart after a successful mutation returns the stored outcome without repeating broker I/O.
3. Kill the process after simulated broker acceptance but before acknowledgement: startup atomically records `recovered_unknown`, never resubmits, and preserves the original key.
4. A recovery audit write fails: roll back the state change; a later successful startup records recovery exactly once.
5. Expire a queued mutation while another operation is blocked: return `request_expired`, create no durable intent, and never execute the expired request after the worker resumes.
6. Time out an already dispatched mutation: return `outcome_unknown`; eventual completion remains inspectable using the original key.
7. Fill the bounded queue: reject excess work promptly, retaining functional health, readiness and authenticated metrics.
8. SIGTERM during broker I/O: stop admission and wait for the active operation's durable completion before exiting.
9. Audit intent write fails: no financial mutation occurs; readiness becomes unavailable while authenticated metrics expose the failure.
10. Detect audit corruption at runtime or startup: refuse further financial admission; do not append to a known-invalid chain.
11. Revoke access to one participant in a transfer: hide its audit events and stored outcome even from the original subject.
12. Invalid arguments, conflicts and replays: record safe codes and correlation identifiers without input bodies or secrets.
13. Provider errors containing secret-like text: emit only sanitized operational warnings, correlated to the request.
14. Metrics without authentication, without admin scope, or with restricted account grants: deny access; exported metrics contain no account/subject/key labels, and concurrent histogram scrapes preserve consistent buckets/count/sum.
15. Hundreds of health probes: do not exhaust ordinary API request limits or consume financial queue capacity.
16. Zero/excessive request deadlines or a queue deadline exceeding the request deadline: reject configuration before serving.
17. Partial entry, cancellation with remaining inventory, final exit, late fees and duplicate snapshots across long/short positions: incremental projections equal an independent full ledger replay after every committed operation.
18. Compact cash reads preserve ordering, currencies, account isolation and soft deletion, and reject unrepresentable amounts and malformed trade identities.
19. A typed funding rejection survives the service boundary as a definite failure, with unchanged cash and no broker request.
20. Fee, distribution and transfer workers nested inside a facade savepoint: preserve outer atomicity on file-backed SQLite, including failures after earlier ledger/projection writes.

Evidence: `service/tests/e2e.rs`, `service/tests/service_contract.rs`, the core projection differential test, and the SQLite cash-movement regression. The gateway is a local simulator; these cases do not establish real IBKR session or exchange behavior.


## Owner authority and stop protection

1. Missing role defaults to agent; zero or multiple owner grants fail startup.
2. Agent grant contains policy, administration or manual-accounting scopes: fail startup.
3. Agent token claims owner status, extra privileged scopes or an authentication deadline: role remains the configured agent and all owner actions are denied before operation claim.
4. Owner login evidence is missing, wrong-context, future-dated or stale: HTTP/MCP challenge for human MFA; no financial mutation.
5. Access-token refresh updates issuance/expiry but preserves old login time: owner elevation stays expired.
6. Owner authentication expires before dispatch or replay: deny without claiming/replaying a financial operation; fresh authentication may reuse the original key.
7. Owner client requests only read/trading scopes: ordinary scoped workflow remains available without elevation.
8. Long stop moves down, short stop moves up, or either gives back locked profit: deny for both roles before broker I/O.
9. Current or proposed stop is nonpositive: deny; invalid existing protection requires reconciliation.
10. Caller uses a stale trade snapshot after an earlier tightening: validate against the latest persisted stop.
11. Valid stop amendment is replayed with its original key: return the stored result without another broker mutation.
12. Agent funds a prepared trade: reserve existing capital under owner limits; never credit new ledger cash.
13. A weaker broker snapshot follows confirmed or externally observed tighter protection: retain observed state for accounting but never lower the amendment bound from verified history.

## Observability and performance

1. A blocked stdout collector fills the bounded log queue: producers still finish; dropped-line telemetry increases. Financial journaling does not use this queue.
2. Submillisecond work: duration sums preserve microseconds and histogram bucket boundaries remain consistent during concurrent scrapes.
3. Arbitrary caller tool names and paths: use fixed `unknown` classes, never add labels or expose input values in metrics.
4. Replay, authorization denial and normal execution: count each attempt, but only actual domain work enters the domain-stage histogram; replay never repeats a deposit.
5. Monitoring credentials lack MFA and carry extra issuer scopes: only the configured `trust:observe` grant applies; no financial tool or account data is accessible.
6. Monitoring scope missing from the access token, or an account-restricted monitoring grant: deny global metrics.
7. IBKR success and error attempts: emit correlated endpoint-class timings without broker identifiers, URLs or sensitive response text; preserve the single-attempt mutation behavior.
8. Concurrent HTTP/MCP previews: keep risk validation, audit every read, verify the resulting chain and submit no broker orders; measure response-body completion and enforce the release latency gate.
9. Stop and target acknowledgements are both lost: reconcile their intents independently, whether observed together or separately; repeated snapshots do not duplicate confirmation evidence.

## Shared broker-account safety

These cases extend the register. A listed boundary is a requirement; use the named-test evidence and verification record to distinguish executed cases from broker/deployment acceptance.

1. Two Trust profiles share the same broker/environment/native account: reservations consume one common budget, even when different agents fund concurrently.
2. A different environment or broker has the same account text: scopes remain distinct; missing/invalid native identity blocks new exposure.
3. External long/short positions consume absolute gross exposure, including positions unknown to Trust.
4. Manual working orders have bounded price/quantity, no price, unknown type, missing currency, duplicate ID, pending cancellation or partial fill: no unbounded exposure is treated as zero.
5. Known bracket children count as exits only when identity, instrument, direction and remaining quantity reduce confirmed exposure; unrecognized siblings do not receive that exemption.
6. Missing, inactive, retyped or weakened safety stops block admission. Long/short protection compares the strongest audited stop, including external observations retained by financial audit.
7. A fill appears between position/order reads: inconsistent evidence blocks admission; timestamps are broker source times, not just receipt times.
8. Position pages truncate, repeat an instrument, hit the cap or fail mid-read; order snapshots are refresh acknowledgments rather than complete results.
9. Snapshot is stale, future by less than a second, older than the last accepted observation, incomplete, foreign-account or foreign-currency.
10. Option/future/bond/unknown multiplier or mixed currency appears: no implicit stock multiplier or FX aggregation.
11. Price, quantity or aggregate exceeds Decimal range: fail explicitly; never substitute zero, saturate money or fall back to float.
12. Submission credits only its own funded reservation; another profile's reservation remains committed.
13. Available broker funds are lower than local ledger capital or are already reserved by other drafts.
14. Readiness is clear but new-notional capacity or mandate slots are zero; a preview never constitutes execution permission.

## Persistent halt, equity and mandates

1. Unknown financial operation affects a secondary transfer/distribution account: all affected broker scopes remain blocked.
2. Crash before/after intent, broker acceptance, financial write or completion record: retain uncertainty without replay; original keys stay bound to original arguments.
3. A failed read, advisory request or journal-only mutation must not invent unresolved financial exposure. Journal-only uncertainty remains inspectable.
4. Refresh succeeds, profit recovers, policy changes or the service restarts after a halt: none silently resume new exposure.
5. During a halt, authorized protection, cancellation and exit workflows remain available with their existing lifecycle restrictions.
6. Owner resume with unresolved operations, stale data, breached loss/exposure or missing Trust entry/protection fails; an agent cannot resolve or resume.
7. Current safety row is altered, deleted or rolled back while historical evidence remains: fail closed; state/history/audit writes roll back together.
8. Resolution evidence is altered or its audit reference is missing: it cannot hide the original unknown operation.
9. A caller loses access to one linked profile: shared policy/halt/recovery checks fail before claiming or replaying an operation.
10. Unrealized losses reach a daily/drawdown limit exactly; high-water history survives restart and later gains.
11. UTC rollover occurs between observations, including an overnight loss; rolling the date must not forgive the unseen loss.
12. External deposit/withdrawal, duplicate cash-flow reference, unexplained equity/P&L difference and broker P&L counter reset: distinguish evidenced cash flows from trading performance.
13. Owner counter reconciliation with zero cash flow retains losses; a reset cannot erase drawdown or clear a halt.
14. Initial baseline begins after earlier same-day losses: explicitly document the missing earlier history and require operational review.
15. Agent mandate missing, expired at dispatch, instrument absent, short disallowed, order/aggregate budget exhausted or open-trade count reached.
16. Existing agent positions appreciate: marked exposure cannot remain valued only at original entry; another agent's allocation does not reset the shared cap.
17. Agent tries funding/submitting a draft prepared by another subject; owner policy authority does not change draft ownership automatically.
18. Background poll duration across all scopes exceeds freshness budget; broker pacing, outages and disk delays must fail closed and trigger independently observed alerts.

## MCP, identity and acceptance evidence

1. Every result validates against its specific output schema, including pagination, decimal strings, nested financial records, errors and original operation results.
2. Suggested actions reflect caller grants, account/broker support and trade state; withheld snapshot details cannot leak another agent's records.
3. Prompt identifiers are malformed, cross-account, untrusted instructions or a trade without its account: reject invalid context.
4. A trade note claims system authority, asks for weaker stops, policy changes or owner credentials: treat it as untrusted data.
5. A source-blind agent must discover capacity, size a requested oversize trade, preserve uncertainty after a lost response and distinguish acceptance from filling.
6. Owner has no enrolled credential, WebAuthn verification is absent, an SSO session exists, requested ACR is lower, or PKCE is missing: no privileged token.
7. Enrollment/password-only flow receives a valid issuer token but no Trust audience; agent client credentials and replayed assertions cannot gain owner authority.
8. Owner auth_time expires while access-token exp remains valid; replay and queued dispatch still require fresh human authentication.
9. Physical key lost, spare unavailable, issuer key rotated, admin ingress exposed or restored issuer database stale: real deployment acceptance, not inferred from virtual WebAuthn.
10. Paper evidence has a duplicate/missing fill, unmatched fee, changed account/currency, pending commission, stale capture, changed source or residual order/position: reconciliation cannot pass.
11. Trust cash projections are compared to independently exported opening/closing broker cash using exact fills and fees; ledger reservations are not mistaken for broker cash.
12. Collector pagination changes mid-read or exceeds its budget; artifacts fail their hashes or a run attempts to overwrite old evidence: stop without a broker mutation.
13. Successful reconciliation for one long one-unit run does not certify partial fills, OCA behavior, actual reconnect, production authentication or complete assurance level 4.


## Encrypted storage and secret custody

1. Missing, malformed, reused or wrong database keys must prevent startup; plaintext files cannot be mistaken for keyed storage. The native backend must be patched stable SQLCipher 4, never ordinary SQLite or an unreviewed major format.
2. Symlink/hardlink/FIFO/device secrets, public files, writable parents, relative/traversing key paths and keys inside either data directory are rejected. Production keys require RAM-backed mounts; host swap encryption remains an operator obligation.
3. Financial/control keys are distinct from backup keys. Ciphertext tampering and wrong backup keys must fail without importing partial financial state. The original source and existing destination must survive a refused copy unchanged.
4. Rotation/migration locks out a running service, creates a new encrypted destination and preserves schema/version/audit semantics. An encrypted backup must restore balances after a simulated filled-and-closed lifecycle. Restart must retain idempotency.
5. Logs must not contain raw keys or tokens. Encrypted databases/WAL must not expose fixture account names or the SQLite plaintext header. Container startup must demonstrate private RAM keys under the production UID.
6. Whole-file rollback, lost escrow, old plaintext snapshots, encrypted-volume readiness, physical-key recovery and host/root compromise require operational acceptance; page encryption alone cannot establish those properties.

## 21. Prospective broker-account risk (PORTFOLIO)

These cases are covered by `core/tests/portfolio_risk_test.rs`, strict IBKR adapter tests and the service MCP process tests. Simulated evidence does not replace paper acceptance.

1. Two agents/profiles reserve against the same equity basis concurrently: only requests fitting shared remaining heat succeed; restart preserves the winner's reservation.
2. Exact cap and one decimal increment above it: heat, stress, margin and concentration remain inclusive/exclusive as specified; overflow or underflow never becomes zero risk.
3. Funding followed by submission: charge once; recompute current constraints and preserve the stronger frozen volatility floor. A failed submit cannot release its earlier funding reservation.
4. Unknown broker outcome, missing trade or unresolved terminal state: retain reservations and halt; no automatic replay or release based on a timeout.
5. Manual long/short exposure, unknown instruments, missing/stop-limit/partial/overlapping protection: use conservative long loss or reject; one broker order cannot protect multiple Trust legs/trades.
6. Partial fills: sum marked open quantity and actual remaining entry; do not double count local funded state. Include external working entries in liquidity participation.
7. Margin amount, segment, timestamp, currency or availability missing: reject. Commodity deficit cannot be offset by positive securities liquidity; maintenance stress and pending full notional consume cushion.
8. Instrument, sector and correlated group limits: aggregate gross exposures across profiles, with no diversification credit from opposite directions or agent-supplied sector labels.
9. ATR rises, proposed stop is very tight, fees/slippage increase, liquidity shrinks: permitted size cannot increase. A later quieter quote does not erase the reservation floor.
10. Delayed/frozen/wrong-contract/future/stale quotes, crossed/empty book, repeated/out-of-order/current-day/incomplete/stale history, inconsistent volume units: reject instead of inventing usable market data.
11. Gap beyond the stop and simultaneous adverse moves: separate stress budget and margin buffer bind even if simple stop risk fits. Finite stress does not guarantee an execution loss bound.
12. Entry-window boundary, earnings blackout and profile expiry: funding and submission use current UTC policy. No automatic liquidation or flat-by-time claim for existing positions.
13. Equity loss/drawdown approaches hard limits: prospective heat shrinks with remaining headroom; throttle decreases immediately. Recovery cannot jump several steps after downtime or ignore its delay.
14. Policy edits, cancellation/re-entry, fresh keys and restart: daily agent loss, entry and turnover usage persist. Outstanding commitments carry through UTC midnight; actual adverse exits and late fees are counted.
15. Fresh account read or sizing request: `risk_check` observes broker evidence without a draft/reservation/safety-state write. Return source timestamps and exact blockers; redact other agents' exposure breakdowns.
16. Nested policy money as floating JSON numbers, exponent/overprecision strings, invalid dates, wrong object/array shapes or unknown keys: reject before domain dispatch.
17. A high win rate with negative net evaluation-window performance: no level upgrade/recovery, even below hard loss thresholds. Loss-triggered downgrades retain priority.
