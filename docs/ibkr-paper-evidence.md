# IBKR paper lifecycle evidence

This workflow prepares real-broker acceptance. It has **not** been run against an authenticated IBKR gateway for this revision. The collector only reads Trust; it cannot submit, cancel, close, resume, reconcile or modify policy. Run the offline assurance levels first and complete `make ibkr-paper-preflight` against an explicitly configured paper gateway before authorizing a supervised order.

Use an isolated paper account with no existing position or working order, one long stock/ETF unit, an explicit notional ceiling, and existing owner-configured risk policy/mandate. Never use this narrow checker to certify a mixed portfolio, short trade, bond, derivative, FX conversion or unrelated cash activity. The existing `scripts/ibkr_paper_acceptance.py` remains a separate submit/cancel smoke; it does not demonstrate fills or protection.

## Collect four observations

Authenticate a human owner session with grants for every linked Trust profile. Store the short-lived token in a private file outside the repository. Refresh the broker and reconcile through the authorized MCP workflow **before** each capture. A capture never repairs stale evidence automatically. Keep the source revision unchanged throughout a run, including skills and test scripts.

```bash
python3 scripts/ibkr_paper_evidence.py capture \
  --service-url https://trust.example.com \
  --token-file /secure/owner-access-token \
  --account-id ACCOUNT_UUID --trade-id TRADE_UUID \
  --stage before --output .local/paper-run/before.json
```

Repeat the command with each stage and a different output filename. Files are created exclusively with mode `0600`; the collector refuses to overwrite a prior observation. Renew owner authentication through the human client as needed. No token is retained in the capture.

| Stage | Required observation and independent evidence |
| --- | --- |
| `before` | Fresh, unfunded `New` one-unit bracket; broker account flat. Retain original broker opening cash/position evidence. |
| `protected` | Authorized entry actually filled. Retain broker entry execution, child activation, parent IDs, OCA linkage, stop price/type and exact quantities. A stop acknowledgment without activation is insufficient. |
| `reconnected` | Supervise a service/gateway restart with existing protection. Reauthenticate/reconcile and retain the original broker order IDs, fresh positions and children. Preserve restart/session logs separately; the collector cannot prove a restart occurred. |
| `closed` | An observed target or stop fill, terminal sibling, flat broker positions and no working orders. Wait for final fees and export executions, commissions, cash and positions independently from IBKR. Never manufacture a fill or use manual accounting to make the test pass. |

Perform order actions in the human-approved MCP trading workflow with stable keys. On a timeout, inspect the original operation and keep the account halted until the real outcome is established. Do not automatically cancel or liquidate unrelated orders as cleanup. A failure to observe the intended target/stop/partial-fill scenario is inconclusive.

## Independently reconcile

Normalize the original IBKR export into a local JSON statement. Preserve the original export unchanged and record its SHA-256. This is an operator normalization step, not a second export of Trust's ledger. The verifier cannot authenticate the export or prove the normalization was faithful; those remain reviewer responsibilities.

The statement contract is:

```json
{
  "schema_version": 1,
  "complete": true,
  "broker_account_id": "DU_PAPER",
  "currency": "USD",
  "period_start": "2026-10-27T12:00:00Z",
  "period_end": "2026-10-27T16:00:00Z",
  "opening_cash": "1000.00",
  "closing_cash": "1008.00",
  "opening_positions": [],
  "closing_positions": [],
  "closing_working_orders": [],
  "external_cash_flows": [],
  "fees_final": true,
  "executions": [
    {"broker_execution_id":"BROKER_FILL_1","broker_order_id":"BROKER_ENTRY","account_id":"ACCOUNT_UUID","trade_id":"TRADE_UUID","symbol":"TEST","side":"Buy","qty":"1","price":"100.00"},
    {"broker_execution_id":"BROKER_FILL_2","broker_order_id":"BROKER_TARGET","account_id":"ACCOUNT_UUID","trade_id":"TRADE_UUID","symbol":"TEST","side":"Sell","qty":"1","price":"110.00"}
  ],
  "fees": [
    {"id":"BROKER_FEE_1","broker_order_id":"BROKER_ENTRY","currency":"USD","amount":"1.00"},
    {"id":"BROKER_FEE_2","broker_order_id":"BROKER_TARGET","currency":"USD","amount":"1.00"}
  ]
}
```

These are illustrative values, not broker evidence. Map the broker account/trade references to the exact Trust UUIDs after checking native IDs. All money and quantities are plain decimal strings. Include all fills and fees for the period; the narrow run refuses unrelated activity. `fees_final: true` requires independent confirmation, including an explicitly confirmed zero-fee outcome.

Create `manifest.json` beside the artifacts. Each entry names a local file and its actual SHA-256 (`sha256sum FILE`):

```json
{
  "before": {"path":"before.json","sha256":"ACTUAL_SHA256"},
  "protected": {"path":"protected.json","sha256":"ACTUAL_SHA256"},
  "reconnected": {"path":"reconnected.json","sha256":"ACTUAL_SHA256"},
  "closed": {"path":"closed.json","sha256":"ACTUAL_SHA256"},
  "statement": {"path":"statement.json","sha256":"ACTUAL_SHA256"},
  "broker_sources": [{"path":"original-ibkr-export.csv","sha256":"ACTUAL_SHA256"}]
}
```

```bash
python3 scripts/ibkr_paper_evidence.py verify \
  --manifest .local/paper-run/manifest.json \
  --output .local/paper-run/reconciliation.json
```

The checker requires matching account/currency/source, ordered fresh observations, one-unit entry/exit, retained protection identities across reconnect, terminal sibling, flat final broker state, exact execution IDs/sides/quantities/prices, matching aggregate ledger fees and cash conservation. Cash tolerance is fixed at `0.01` major currency units; average fill-price rounding tolerance is `0.000001`. It rejects duplicates, non-decimal financial values, pending fees, unknown operations, incomplete pagination and hash mismatches. Ledger reservations are deliberately not compared to broker cash; the cash check uses independent buys, sells and fees.

A reconciled run **does not promote assurance level 4**. Review original broker OCA/parent linkage, real stop activation, session/restart logs, normalization and final fee timing. Repeat target and stop paths and retain partial-fill observations, cancel/fill races and remaining [acceptance cases](reliability/assurance.json). Operational level 5 also requires alert delivery, storage/backup recovery and repeated authentication/session rollover. Neither paper success nor a passing script establishes live execution quality or a guaranteed loss bound.
