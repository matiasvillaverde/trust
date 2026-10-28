# Research and analysis

Use only the evidence needed for the user's decision. Read-only research remains read-only; tools such as catalyst scans and journal writes mutate even if they sound analytical. Get exact schemas from discovery rather than inventing parameters.

| Need | Tools and interpretation |
| --- | --- |
| Registered contract and metadata | `instruments_list`, `instruments_stats`; an owner uses `instrument_resolve`/`instrument_update` for missing mappings. A ticker is not a unique contract. |
| Price context | `market_snapshot`, `market_quote`, `market_trade`; retain source, currency and observation time. A fallback price can be stale; unavailable is not zero. |
| Historical context | `market_bars` with bounded dates and supported timeframe. Never fabricate bars for correlation or regime. |
| Session/event context | `market_session` is snapshot context, not an exchange-calendar guarantee. `market_stream` is a finite event batch, not continuous monitoring. |
| Portfolio risk | `report_get` with `risk`, `concentration`, `drawdown` or `summary`; distinguish ledger reports from fresh broker-wide `account_readiness`. |
| Supplemental advice | `advisor_status`, `advisor_check`, `advisor_correlation`, `advisor_regime`, `advisor_history`; advisory history is process-local. |
| Performance/fixed income | `metrics_get` for `advanced`, `compare` or `bond`; comparisons require access to both accounts and explicit matching currencies. Instrument analytics do not establish execution support. |

A concentration result labeled `Block` is an advisory model result, not the service's admission decision. Explain it and follow the user's mandate. Missing advisory data is unavailable, not success or an invented market regime. Account rules, whole-account limits and the caller's mandate remain the enforcement authority.

Example read-only quote request after exact account selection:

```json
{"tool":"market_quote","arguments":{"account_id":"11111111-1111-4111-8111-111111111111","symbol":"TEST"}}
```

If denied or unavailable, report that limitation. Do not create a trade or change market-data permissions to manufacture the requested analysis. A security reported as tradable by a broker can still be unsupported by Trust's whole-account risk controls, currency or multiplier checks.

For a requested bar interval absent from the schema, explain the limitation. Derive coarser bars only from sufficiently complete finer data, with explicit timezone, interval boundaries and missing-data handling; label them derived. Never silently substitute another interval or invent missing minutes. A previous weekday is not a verified previous trading day: establish the exchange/calendar or label the date assumption. A generic market-data rejection does not establish its cause. Preserve the request ID for operator diagnosis; connectivity or trading readiness alone does not prove data entitlement or coverage.

Finish with evidence dates, relevant constraints, assumptions and unavailable facts. Use the service's financial calculations; avoid representing planned risk as a guaranteed loss bound. For tax/wash-sale reports, clarify full-ledger coverage and that estimates are not filing instructions.
