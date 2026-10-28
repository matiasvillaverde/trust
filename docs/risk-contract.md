# Capital preservation and adaptive sizing

Trust controls how much risk an agent may take. The owner sets the limits; the service calculates admissible size, reserves capacity, checks again before submission, and blocks new exposure when the evidence or policy does not permit it. Agents propose trades and manage them within that authority.

The product goal is to prevent agents from exhausting an account through oversized or repeated risk-taking. This is an engineering requirement, not a promise that markets cannot produce losses beyond a stop. Stop prices are not guaranteed execution prices; margin can create losses exceeding invested funds. See [FINRA's stop-order guidance](https://www.finra.org/sites/default/files/notice_other_file_ref/Regulatory-Notice-16-19.pdf) and the [SEC's margin-account bulletin](https://www.investor.gov/introduction-investing/general-resources/news-alerts/alerts-bulletins/investor-bulletins-29).

## Owner ceiling, performance adjustment, admission

These controls serve different purposes:

1. **Owner ceilings:** hard limits on risk and exposure. A profitable history, an agent mandate or a favorable forecast never overrides them. Changes require the separate owner identity with recent human MFA.
2. **Performance adjustment:** qualifying losses reduce the position allowance. Qualifying recovery can restore it within the ceiling. Neither a single winning trade nor an agent's confidence is sufficient evidence for an increase.
3. **Admission:** the persisted trade must fit current rules, available capital, broker exposure and the caller's mandate at funding and again at submission. A prior preview is not a reservation or permission to ignore changed conditions.
4. **Persistent halt:** unresolved broker outcomes, stale or inconsistent evidence, and observed account loss breaches stop new exposure. Protection and permitted risk reduction remain available. A restart, another agent or a new Trust profile cannot clear the shared halt. Owner recovery requires verified conditions.

These rules belong in the service and core. Skills explain how to use them; an agent's willingness to follow a prompt is not the enforcement boundary. Broker credentials must stay outside agent sessions. Direct manual trading at the broker remains outside Trust's admission gate and has to be detected through reconciliation.

## What adaptive sizing does today

Local sizing uses available ledger capital and the effective per-trade and monthly rules. Multiple rules use the stricter constraint. Performance levels apply these multipliers to that base quantity:

| Level | Recorded multiplier | Effective allowance relative to the calculated base |
| --- | --- | --- |
| 0 | 0.10 | At most 10% |
| 1 | 0.25 | At most 25% |
| 2 | 0.50 | At most 50% |
| 3 | 1.00 | At most 100% |
| 4 | 1.50 | Still at most 100%; the owner ceiling takes precedence |

Funding and submission enforce the level-adjusted limit as well as the base limit. Losing capital reduces the available basis; qualifying loss breaches also reduce the level. Growth in available capital or recovery from a reduced level can increase size without changing the configured percentage ceiling.

Level evaluation uses recent realized trade performance. It is separate from broker-equity loss and drawdown checks, which include unrealized P&L. The existing level policy also has an exceptional-performance cooldown: size is not a function of the last trade's win/loss alone. Loss breaches override ordinary upgrades, exceptional-performance cooldown and cooldown recovery. Their downgrades are exempt from stabilization limits. At level 0 a loss breach cannot trigger an upgrade, but **level 0 is a small allowance, not a trading halt**.

The sizing result is authoritative. Clients must not multiply it by the level again, increase quantity to recover a prior loss, divide a proposal across agents to evade limits, or weaken an existing protective stop.

## Broker-wide prospective risk

The service now requires an explicit owner `controls` policy and applies it at funding and submission across every profile sharing the broker/environment/account. Old stored policies still deserialize, but cannot authorize new risk until the owner supplies the new fields. Agents also need an explicit daily `risk_budget`.

| Control | Enforced behavior |
| --- | --- |
| Portfolio heat | Sum modeled loss from marked positions, working entries and durable funding reservations, including manual activity and every agent. Limit = minimum of equity × heat percentage, remaining daily loss limit and remaining drawdown limit. |
| Trade heat | Candidate modeled loss must fit current broker equity × trade percentage × drawdown multiplier. |
| Margin cushion | Use conservative aggregate/securities IBKR excess liquidity, subtract stress loss, full pending notional and the configured increase in maintenance requirements; require the equity buffer to remain. Missing segment evidence or commodity margin exposure blocks admission. |
| Concentration | Gross instrument, sector and correlation-group notional limits use owner metadata keyed by broker-native ID. No offset/diversification credit; missing or expired profiles block. |
| Gap stress | Simultaneous adverse movement for every holding/entry, at least the instrument gap percentage or its modeled unit risk, plus costs. Stress must fit its separate equity percentage budget. |
| Volatility and execution | Real-time exact-contract quotes; price collar, spread and age limits; ATR floor from completed daily bars; minimum percentage risk distance; book/volume participation including other pending entries; modeled fees/slippage. |
| Drawdown throttle | Broker-equity decline from its cash-flow-adjusted high water reduces new trade heat and order-notional allowances immediately. Recovery advances only one configured increment per verified delay. |
| Agent activity | UTC daily net loss includes filled cash flows, marked remaining quantity and actual fees/tax transactions. Funding charges round-trip turnover and entry count; actual newly observed turnover can charge more. Pending commitments survive rollover. Re-entry delay applies after commitment and after an observed close. |

`risk_check` obtains fresh broker/margin/market evidence and returns a conservative admissible quantity within the requested ceiling without creating a draft or reserving capacity. `risk_preview` remains the local ledger/level calculation; `trade_hypothesis.max_loss` remains the simple stop estimate. Neither replaces full admission. Decimal calculations are checked; overflow, missing evidence and unsupported currencies/multipliers fail closed.

Per-unit planned loss is the maximum of directional stop distance, owner minimum percentage distance and the frozen volatility floor, plus modeled adverse execution. Fees are added once using the greater of per-unit round-trip costs and the configured minimum. Existing position loss is measured from the broker mark, so a profitable stop above entry does not imply zero risk to current equity. A tighter stop earns credit only from observed protection; its volatility floor cannot silently shrink after funding.

An unprotected external long consumes its full marked value. Unknown short exposure is blocked. A manual position receives stop credit only from one unambiguous active full-position market stop. Stop-limits, multiple overlapping exits, pending cancellations and unknown order relationships are not assumed to guarantee protection. Partially filled trades charge both the marked open quantity and the remaining entry quantity.

The single financial worker persists reservations and their audit/history atomically before financial dispatch. Submission rechecks them without double charging. Definitive local rejection can release its own new reservation; uncertain outcomes retain reservations and halt trading. Reservations are released after verified terminal state and no unresolved operations. The financial database and control journal are separate transactions: interruption between them leaves a conservative unknown operation, not reusable capacity.

Policy changes, new idempotency keys, another agent or another Trust profile cannot reset usage. Cancellation does not erase an accepted entry charge. Old commitments remain charged across midnight; newly observed executions are counted on the observation day, so late evidence is conservative. Initial positive agent P&L establishes a baseline and grants no historical-profit credit; initial negative P&L and newly observed historical turnover are charged conservatively.

## Limits of the model

These are deterministic admission controls, not a guarantee against ruin. Stops can gap, liquidity can disappear, short losses can exceed any finite scenario, and broker liquidation rules can change. The margin calculation is a conservative supported-asset approximation, not IBKR's portfolio-margin simulator. `broker_preview` remains useful supplemental what-if evidence. No automatic FX or derivatives multiplier is inferred.

Owner entry windows, blackouts and metadata expiry can restrict entries around earnings, overnight periods and weekends. They **do not automatically liquidate existing holdings by a deadline**. Flat-by-time requirements still need a separately authorized, verified exit workflow and operator escalation for failed exits. Correlation groups are owner-defined, not automatically calibrated historical correlations. History freshness does not prove exchange-session completeness; real-broker qualification must verify the chosen bar window, corporate actions and contract/volume conventions.

External/manual trading cannot be reserved atomically with Trust: the broker can change immediately after a snapshot. Fresh evidence and the existing bounded-consistency checks reduce that race but cannot remove it. Only one worker may own the financial/control stores; active-active writers are unsupported. A quote history, policy or equity observation is evidence at its timestamp, not a promise about the next execution.

Level 0 remains reduced sizing, not a halt. Losing level-evaluation windows cannot upgrade even when their win rate is high. Broker-equity throttling and portfolio admission still apply independently of local levels.

## Verification and deployment

Deterministic and property tests exercise aggregate boundaries, manual exposure, partial fills, gap/fee/margin constraints, concentration, stale/delayed evidence, volatility floors, recovery timing, midnight rollover, activity limits, arithmetic failure and restart persistence. Process-level MCP tests race two agents across linked profiles, verify read-only admission previews and recheck changed quotes/margin/calendar before broker submission. See the [verification record](reliability/verification.md) for actual commands and limits of the evidence.

Offline tests cannot prove real IBKR fills, protective routing, gap execution or cash/position reconciliation. The [paper acceptance evidence](ibkr-paper-evidence.md) and operational drills remain prerequisites to a separately authorized live canary. No skill or test fixture supplies owner policy defaults or marks broker-connected acceptance complete.

Implementation sources: [IBKR summary fields](https://ibkrcampus.com/docs/web-api/api-reference/trading/trading-portfolio/get-portfolio-summary), [IBKR market-data fields](https://www.interactivebrokers.com/docs/web-api/v1/endpoints/market-data/market-data-fields), [IBKR excess liquidity](https://www.interactivebrokers.com/campus/glossary-terms/current-excess-liquidity/), [QuantConnect risk models](https://www.quantconnect.com/docs/v2/writing-algorithms/algorithm-framework/risk-management/supported-models), [FIA automated-trading controls](https://www.fia.org/sites/default/files/2024-07/FIA_WP_AUTOMATED%20TRADING%20RISK%20CONTROLS_FINAL_0.pdf) and [Schwab ATR](https://www.schwab.com/learn/story/average-true-range-indicator-and-volatility). These inform the design; their publication is not evidence that Trust's implementation is broker-qualified.
