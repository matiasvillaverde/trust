# Verified recovery, audit and backups

Use this procedure in the separate human owner session. Owner authentication is necessary but cannot substitute for evidence. Keep the trading agent's credential unchanged.

## Resume a halted account

1. Read `account_readiness`, unresolved operation IDs and the affected trades. `operation_get` is scoped to the original subject/key; an owner's lookup under a different subject does not reveal that agent's operation. Use the original agent's handoff, owner-visible readiness and audit evidence rather than switching the agent to an owner credential.
2. Refresh a complete broker snapshot, reconcile all affected trades/pages, and independently verify orders, executions, positions, fees and cash. Empty or incomplete IBKR cache responses are unavailable evidence. Do not reset a policy or record a made-up fill to clear blockers.
3. If a broker P&L counter reset is independently verified, use `account_equity_reconcile` with a unique evidence reference and explicit `cash_flow_amount` (`"0"` if none). Real external deposits/withdrawals use `account_cashflow_record` with a unique reference and signed amount. Preserve actual losses and high-water history. Earlier losses before the first accepted baseline need separate review.
4. Use `operation_resolve` only after evidence establishes the outcome and persisted trades match fresh broker state. Resolution remains recorded and non-replayable; it does not execute the original request. A mismatch or missing fees/protection means recovery is incomplete.
5. Request `account_resume` with a specific recovery reason only after unresolved outcomes and all safety blockers are addressed. The service refuses stale/incomplete evidence, breached limits and unreconciled exposure. Verify readiness after the response; do not submit a trade merely to test resumption.

Each refresh, reconciliation page, resolution and resume is a separate intent with its own stable key. If one becomes uncertain, inspect that original operation before proceeding. A previous halt does not prohibit lifecycle-valid protection or exits under their existing permissions.

## Audit and manual evidence

`audit_list` exposes visible service authorization/operation entries; `audit_verify` verifies the durable service chain; `trade_audit` verifies financial history. Compare retained external head hashes or archived logs to detect privileged rewriting/truncation; local hash-chain verification alone cannot establish that.

```json
{"tool":"audit_list","arguments":{"after":0,"limit":20}}
```

Use pagination to investigate a complete time window and correlate request, operation, account and trade IDs. Do not export credentials or raw sensitive payloads. Operational metrics/logs complement the durable audit; they are not fill evidence.

`trade_record_fill`, `trade_record_stop` and `trade_record_target` require owner role, recent MFA and separate `trust:manual` permission. They record evidenced actual execution prices and fees in the ledger without sending a broker order. They are not a shortcut for uncertain broker actions or a way to bypass stop protection.

## Backups

`backup_export` creates a SQLCipher-encrypted financial artifact using the separately mounted backup key and returns an opaque artifact ID. Full disaster recovery also needs the control journal, configuration and securely retained encryption keys; coordinate a stopped-worker backup with the operator. Never ask the agent to transmit those keys.

`backup_restore` imports a server-owned encrypted artifact into an empty financial database. Replacing a running ledger is an offline operator procedure: restoring data cannot roll back broker orders. Confirm restore scope and active broker exposure with the owner before this mutation; no arbitrary file path is accepted. Read back the result and verify audit/reconciliation before any resumption.

Finish with evidence sources/timestamps, resolved and unresolved operations, protection/exposure, verified audit head, artifact IDs if applicable and remaining recovery actions. Account sizing levels are unrelated to staged acceptance evidence; offline success, an SDK sign-in or a paper submit/cancel cannot certify real fills, fees, reconnects or live readiness.
