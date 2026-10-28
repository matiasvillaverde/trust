# Sessions, journals and review

These workflows support trading discipline; journaling permission does not grant policy or trading authority. Read-only review uses read/preview tools. Persist plans, events or grades only within the requested workflow and with a stable key for each mutation.

## Session planning

Inspect `sessions_list` before opening a plan. Do not duplicate an active session. `session_open` accepts regime `calm`, `normal` or `elevated`, nonblank setup names unique ignoring case with no commas, and a hypothesis no longer than 500 characters. If observations are missing, state that limitation; do not claim the chosen planning regime was measured. The session plan does not replace enforced mandate limits.

```json
{"tool":"session_open","arguments":{"account_id":"11111111-1111-4111-8111-111111111111","regime":"normal","setups":["breakout"],"max_positions":1,"hypothesis":"Example plan only; current market evidence is unavailable.","success_criteria":"Respect the mandate and verify protection.","failure_criteria":"Stop new risk on uncertain broker outcomes.","idempotency_key":"example-session-01-open"}}
```

Use `session_close` with adherence notes and grade after reviewing the session. A session's close is not a position exit.

## Choose only the needed tools

| Task | Tools and completion evidence |
| --- | --- |
| Inspect catalysts/events | `trade_events_list`; `trade_catalysts_scan` queries configured providers and records events, so it is a mutation. Provider secrets stay with the operator. |
| Record an evidenced event | `trade_event_record`; keep event type/severity/date aligned with the discovered schema and retain source in notes. |
| Review process quality | `grade_preview`, `grade_get`, `grades_list`, `trade_mistakes_list`; distinguish evidence from inference. |
| Persist a post-trade review | After confirmed closure, `grade_record` and `trade_mistake_record`; do not attribute a hypothetical result as an actual execution. |
| Account reports | `report_get`: summary, performance, drawdown, risk, concentration, metrics, attribution, benchmark, timeline, bias, wash_sales or tax. Use supported filters; unavailable data stays explicit. |
| Ledger/distributions | `balances_get`, `transactions_list`, `distribution_history`, `distribution_policy_get`; ledger values are not broker cash snapshots. |
| Level review | `level_get`, `level_history`, `level_policy_get`, `level_progress`; applying/changing level policy is owner-only. |
| Service investigation | `audit_list` for visible entries, `trade_audit` for a trade; full service-chain verification needs owner authority. |

Tax and wash-sale reports cover the available full ledger rather than an assumed filtered tax year; explain their estimated scope. Cross-currency aggregation is rejected without explicit conversion support. Report date windows, missing data, unresolved reconciliation and whether a grade was merely calculated or actually recorded.
