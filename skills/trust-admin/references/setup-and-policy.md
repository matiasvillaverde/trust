# Owner setup, policy and capital accounting

Use the human owner's explicit requested values and fresh MFA; do not invent financial limits to make an agent ready. Discover exact schemas and read current policy before writing. Each mutation needs a unique stable intent key and a read-back check.

## Bring an account to readiness

1. Inspect `accounts_list` and `connections_status`. `account_create` creates a local profile with an explicit broker, paper/live environment and correct broker account binding. Children must match their hierarchy's broker/environment. `account_delete` only removes an eligible empty local profile; its force flag does not authorize removing exposure or bypassing children.
2. Inspect `instruments_list`/`instruments_stats`. Use `instrument_resolve` for the actual broker contract or `instrument_create` plus `instrument_update` with independently verified metadata. Shared catalogue changes require all-account administration. Do not invent a conid, tradability, currency or multiplier.
3. Set both per-trade and monthly rules through `risk_rule_create`, inspecting `risk_rules_list` and deactivating an intended replaced rule with `risk_rule_remove` when appropriate. A zero-percent rule blocks new exposure, including previously funded submissions. Use percentage points, not fractions. Treat intermediate policy states carefully; do not broaden permission to bridge a multi-call setup.
4. Read/set `account_safety_policy_get`/`account_safety_policy_set`: explicit base currency, gross exposure, per-order notional, daily equity loss, drawdown and snapshot age. Limits include external positions, working orders and all linked Trust profiles. Owner access must cover every affected profile. Baselines, drawdown recovery state, reservations and consumed agent budgets are not reset by policy changes. The required `controls` object is described below; legacy policies without it block new exposure.
5. Use `agent_mandate_get`/`agent_mandate_set` for a specific subject, exact instrument UUID allowlist, per-order and total committed notional, open-trade count, short permission and explicit expiry timestamp and required `risk_budget` (daily loss, round-trip turnover, entry count and re-entry delay). Amounts are currency major units. Do not assign owner scopes to the agent.
6. With appropriate authority, refresh broker evidence and reconcile active trades, then inspect `account_readiness` again. Existing halts require the recovery procedure; successful setup is not automatic resumption or authorization to place a test order.

Where a workflow has multiple writes, describe intermediate states and verify each result. Do not imply an atomic transaction across separate MCP calls.

## Prospective controls: explicit owner decisions

`account_safety_policy_set.controls` requires all of the following. Read the live typed schema; values are not defaulted from a model's judgment.

| Controls | Units and meaning |
| --- | --- |
| `max_portfolio_heat_pct`, `max_trade_heat_pct`, `max_stress_loss_pct` | Percentage points of current broker equity; heat is also capped by remaining daily/drawdown loss headroom. |
| `min_margin_cushion_pct`, `maintenance_margin_stress_pct` | Equity buffer and adverse increase in reported maintenance requirements. Pending entries reserve full notional conservatively. |
| `max_instrument_pct`, `max_sector_pct`, `max_cluster_pct` | Gross notional percentages; opposite positions do not create diversification credit. |
| `instruments` | Map of exact broker-native IDs to owner-verified sector, correlation group, gap stress percentage, minimum risk percentage, UTC entry windows/blackouts and expiry. Cover external holdings too. These are static owner groups, not inferred correlation guarantees. |
| `volatility_lookback_bars`, `atr_multiplier`, history/quote ages | Completed daily true-range window and fresh real-time quote requirements. Tight proposed stops cannot evade this floor. |
| Spread/deviation/slippage fields | Basis points; use actual instrument/execution evidence. |
| Volume/book participation | Percentage points of latest completed daily volume and current side book size; includes other pending entries. |
| `fee_per_unit`, `minimum_fee` | Conservative modeled round-trip costs in the base currency. Actual fees still enter realized/unrealized agent performance. |
| `drawdown_steps`, `recovery_delay_seconds`, `recovery_step` | Ordered drawdown percentages with decreasing multipliers starting at 0/1; reductions are immediate, recovery advances at most one step per verified delay. |

`risk_budget.max_daily_turnover` charges a conservative round trip at funding. Repeated cancellation/re-entry consumes budget, outstanding commitments carry across UTC midnight, and newly observed actual fills charge at least their actual turnover. Exits remain allowed when entry budgets are exhausted. Updating a mandate does not reset usage. First observations of pre-existing fills/losses are charged conservatively; do not deploy on an active account assuming a fresh allowance.

Schedules constrain admission, not automatic liquidation. If the owner requires no overnight/weekend holdings, separately arrange and verify authorized exits; this policy does not promise a timed flatten. Missing IBKR segment margin, stale/partial history, unsupported instruments, unresolved outcomes or unverified protection require correction or a handoff, never relaxed checks as a shortcut.

## Capital: distinguish three operations

| User intent | Meaning in Trust |
| --- | --- |
| Record ledger capital | `capital_deposit`, `capital_withdraw`, `capital_transfer`; these change local accounting, not broker cash. Transfers require access to both hierarchy accounts. |
| Move funds at a broker/bank | No Trust MCP tool currently performs this transfer. The human uses the broker/bank's authorized process; never claim a ledger entry moved cash. |
| Account for an actual external cash flow in equity limits | `account_cashflow_record` with unique evidence reference and signed amount; this adjusts the equity baseline, does not write a ledger deposit, and does not clear a halt. |

An illustrative **read-only** verification after an independently authorized ledger operation:

```json
{"tool":"transactions_list","arguments":{"account_id":"11111111-1111-4111-8111-111111111111","limit":20,"offset":0}}
```

Read back ledger balances, transactions and current safety state; record a real event once with the right reference and original operation key. Do not use a deposit or cash-flow record to forgive trading losses.

## Other owner policy

- `level_policy_set` and `level_evaluate` affect risk levels; read `level_policy_get`, `level_history` and `level_progress`. Even evaluation is an owner-scoped tool; inspect its apply/target behavior before calling.
- `advisor_configure` changes supplemental concentration thresholds; read `advisor_status`. Advisory output is separate from enforced admission.
- `distribution_policy_set` uses fractions totaling exactly `1` and an explicit minimum. Its password is mounted by the operator, never supplied by chat. `distribution_execute` allocates realized ledger profit among children; read `distribution_policy_get` and `distribution_history` and verify all account permissions.

Finish with before/after values, units, affected accounts, operation IDs and read-back evidence. An agent requesting larger limits gets a handoff describing the proposed change and rationale; it never executes the owner change itself.
