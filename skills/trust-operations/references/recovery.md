# Recovery: uncertain outcomes and durable halts

An external mutation may have succeeded even when no response arrived. Preserve the original subject, tool, exact arguments, idempotency key and any request/operation IDs. Do not continue the dependent sequence until the prior outcome is established.

## Inspect before acting

For an entry whose original key is `example-intent-01-submit`:

```json
{"tool":"operation_get","arguments":{"key":"example-intent-01-submit"}}
```

Read both the outer tool error and `data.state`, `data.result` and `data.resolution`. A stored completed response can itself contain a rejected financial operation. An owner resolution records evidence; it never replays the mutation or turns an unknown outcome into a retryable request.

| Observation | Next step |
| --- | --- |
| Stored completed result with successful operation | Reuse the result; read current trade/execution state before any dependent action. |
| Stored definitive rejection | Explain the error. A changed proposal is a distinct intent only when within mandate and the original has no unresolved effects. |
| `running` or `outcome_unknown` | Read/reconcile actual broker and ledger state. Do not submit again or rotate keys. |
| `not_found`, denied lookup, missing original key or changed subject | These do not establish absence of an order. Preserve known identifiers and hand off; do not try another subject or invent a replacement entry. |
| `idempotency_conflict` | Recover the exact original request. Do not edit its arguments or change the key to force acceptance. |
| Incomplete/empty IBKR cache-refresh response | Unavailable evidence, not an empty account. Keep uncertainty unresolved. |

Read `trade_get`, `executions_list` and `trade_audit` for the exact account/trade. With scope and task authority, reconcile that trade or the account. These persist state and each needs its own stable key:

```json
{"tool":"trade_reconcile","arguments":{"account_id":"11111111-1111-4111-8111-111111111111","trade_id":"33333333-3333-4333-8333-333333333333","idempotency_key":"example-recovery-01-reconcile"}}
```

For `account_reconcile`, inspect every `data.trades` error and `data.next_after_trade_id`. Continue using `after_trade_id` and a new key for each distinct bounded page. `complete: false` never means successful reconciliation of the whole account. Regular list reads instead use `items` and `next_offset`.

A transport error during reconciliation is uncertainty in that operation too. Stop unproductive retry/poll loops; report the missing evidence and hand off. Do not manually record a fill, fee or cancellation as a substitute for broker evidence.

## Worked lost acknowledgment

The submit call times out after the broker accepts an order. `operation_get` reports `outcome_unknown`. Readiness shows a durable halt. A subsequent order read is empty.

Correct outcome: no replacement submission. Reconcile with available authority, preserve existing stops, and report the unknown operation even if positions currently look empty. The owner must independently establish the outcome, resolve the operation and request verified resumption. Refresh, a new login or a restart cannot clear the halt by themselves.

An evidence-based answer is: “Submission outcome remains unknown for [original key/operation ID]. The last order read at [returned timestamp] was empty; it does not establish nonexecution. I did not resubmit. New risk remains halted. Next: obtain independent broker order/execution evidence for owner recovery.” If the read has no timestamp, say so. “No orders found, so the trade failed” would turn missing evidence into a false conclusion.

When unsafe/unresolved exposure is discovered and halting is within the agent's mandate, `account_halt` stops new exposure without cancelling protection. While halted, only permitted reconciliation and lifecycle-valid risk reduction continue. Never use cancel-all.

## Handoff payload

Give the human the account UUID and paper/live environment, trade UUID, original key and operation ID, last known status, evidenced filled quantity, protection status, broker observation times, exact blockers, and missing order/fill/position/fee/cash evidence. State which action remains unconfirmed. Request independent reconciliation and owner recovery in the human's separate authenticated session; never request an elevated token. Secrets and raw credential-bearing broker messages do not belong in the handoff.
