# Trade entry: sizing through evidenced submission

Use the discovered schemas; the examples below are synthetic calls, not instructions to trade these values. Replace every example UUID with a tool-returned ID and generate fresh keys for a genuinely new intent. Never reuse a sample key or replace the key of an uncertain operation. JSON examples contain a canonical `tool` and `arguments`; your client's call wrapper may differ.

## Sizing without mutations

After confirming the account, broker/environment, registered instrument, currency, policy and caller mandate:

```json
{"tool":"account_readiness","arguments":{"account_id":"11111111-1111-4111-8111-111111111111"}}
```

`account_readiness` is cached evidence: inspect blockers, observation age, `portfolio_risk`, `constraints` and the caller mandate. An existing halt needs verified owner recovery. A strictly read-only task can use `risk_check` to obtain fresh broker, margin and market evidence without changing stored safety state; ordinary read audit records still apply. It never clears a halt, reserves capacity or submits.

For the local ledger/level ceiling:

```json
{"tool":"risk_preview","arguments":{"account_id":"11111111-1111-4111-8111-111111111111","entry":"100","stop":"95","currency":"usd"}}
```

This local ceiling is not full admission. Supply the exact contract UUID, direction, bracket and user-authorized quantity ceiling to the combined check:

```json
{"tool":"risk_check","arguments":{"account_id":"11111111-1111-4111-8111-111111111111","instrument_id":"22222222-2222-4222-8222-222222222222","side":"long","quantity":"10","entry":"100","stop":"95","target":"110","currency":"usd"}}
```

Read `max_admissible_quantity`, `requested_admissible`, `blockers`, `observed_at`, and `admission`. The admission report includes modeled planned loss, stress loss, the minimum unit-risk floor, quote/history timestamps, and portfolio heat, margin and drawdown constraints. Zero quantity or absent admission is not permission to trade. A smaller maximum authorizes only a proposal within the existing mandate; it does not make the original requested quantity admissible. Do not derive full capacity by dividing gross notional by price. Fees, volatility, liquidity, grouped exposure and other agents' commitments can bind first.

If the service returns a maximum of 5 whole shares for the 10-share ceiling, calculate reward at that supported quantity if useful:

```json
{"tool":"trade_hypothesis","arguments":{"account_id":"11111111-1111-4111-8111-111111111111","entry":"100","stop":"95","target":"110","quantity":"5","currency":"usd"}}
```

The simple stop distance implies USD 25 before costs. The admission engine's modeled loss can be higher because of ATR, minimum risk distance, slippage and fees; report both without claiming guaranteed execution. Long geometry is stop < entry < target; short geometry is target < entry < stop. Short collateral uses the service's capital estimate. Currency arguments follow their schemas (`usd` for trade tools, `USD` for account policy); never infer FX or contract multipliers.

**Finish here for sizing or analysis.** For example: “Proposed maximum: 5 whole shares within your 10-share ceiling. [Returned blocker] binds. Modeled loss and stress loss: [returned values/currency]; observed at [returned timestamp]. No trade was created or reserved.” Use actual results, not these illustrative numbers. Never substitute the wall clock for missing source timestamps.

## Execution within an existing mandate

1. Check that the user's instruction/mandate authorizes execution and that all current blockers are resolved. If the request only authorizes a draft, stop after preparation. Session journaling is optional unless the user requires a plan. If `trust:journal` is available and planning is within scope, inspect `sessions_list` before using `session_open`; do not duplicate a plan. Its schema requires a supported regime, short comma-free setup names, and a hypothesis of at most 500 characters. If regime evidence and an authorized planning assumption are both unavailable, explain why the optional plan was skipped; do not invent an observed regime. If the user requires a plan that cannot be completed, stop before entry. Missing advisory evidence alone does not block an otherwise authorized entry. Absent journal permission is not permission to elevate.
2. Create the exact bracket with `trade_prepare`. The returned `data.id` is the trade ID; do not invent it. Persisted initial state is `New`.

```json
{"tool":"trade_prepare","arguments":{"account_id":"11111111-1111-4111-8111-111111111111","instrument_id":"22222222-2222-4222-8222-222222222222","side":"long","quantity":"5","entry":"100","stop":"95","target":"110","currency":"usd","stop_type":"stop","thesis":"Authorized paper example; use verified current evidence in practice.","idempotency_key":"example-intent-01-prepare"}}
```

3. For IBKR, call `broker_preview` with the persisted ID and inspect estimates/warnings. Do not use it for Alpaca. A preview error does not authorize submission; resolve it or hand off. The service's readiness and supported broker capabilities decide whether execution is possible.
4. Call `trade_fund`, which durably reserves capital and shared loss capacity without placing an order; then `trade_submit`, which sends the persisted bracket. Use separate keys for preparation, funding and submission. Check each response before continuing.

The trade UUID below stands for the actual preparation result:

```json
{"tool":"broker_preview","arguments":{"account_id":"11111111-1111-4111-8111-111111111111","trade_id":"33333333-3333-4333-8333-333333333333"}}
```

```json
{"tool":"trade_fund","arguments":{"account_id":"11111111-1111-4111-8111-111111111111","trade_id":"33333333-3333-4333-8333-333333333333","idempotency_key":"example-intent-01-fund"}}
```

```json
{"tool":"trade_submit","arguments":{"account_id":"11111111-1111-4111-8111-111111111111","trade_id":"33333333-3333-4333-8333-333333333333","idempotency_key":"example-intent-01-submit"}}
```

5. With reconciliation authority, call `trade_reconcile` using its own key. Inspect `trade_get` and `executions_list` as needed for actual filled quantity, protection and fees. Without reconciliation authority, report the last confirmed state and the required reconciliation handoff. An accepted submission alone is not a fill.

## Rejections and changing capacity

Readiness can change between preview and funding. For `portfolio_heat_exceeded`, concentration, stress, margin, agent-budget or notional denials, inspect `error`, `operation_get` if needed, and fresh readiness. After a definitive failed reservation, report the constraint or prepare a smaller distinct proposal within the original mandate. Never split across profiles to evade limits, raise policy, modify an existing stop to take more risk, or treat a transport timeout as a definitive rejection. Unknown operations go to the recovery workflow with their original keys.

## Calendar and protection boundaries

Entry windows, earnings blackouts and policy expiry are owner-defined UTC intervals. Do not fabricate an earnings calendar, extend a profile or widen a stop. These schedules block new entries; they do not automatically flatten holdings before a weekend or earnings event. If the user requires flat exposure by a deadline, make the need for an explicitly authorized and verified exit workflow clear. A stop-limit is not evidence of a guaranteed market exit. Unknown short exposure or ambiguous protection can block the account entirely.
