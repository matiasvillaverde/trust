# Interactive Brokers E2E Smoke Test

The live IBKR smoke test verifies that an authenticated Client Portal Gateway can resolve Trust securities across the supported IBKR categories and preview Trust bracket orders with IBKR's non-mutating `/whatif` endpoint:

- stock -> `STK`
- ETF -> `STK`
- bond -> `BOND`

This test does not place orders. It is ignored by default and must be enabled explicitly because it depends on a live authenticated gateway session. The what-if preview calls IBKR's documented [Preview Order / WhatIf Order](https://www.interactivebrokers.eu/campus/ibkr-api-page/cpapi-v1/#preview-order-whatif-order) endpoint, which returns commission and margin impact without submitting the order.

A second, separately gated test submits one stock bracket to an IBKR **paper** account, confirms cancellation, and verifies the local tamper-evident audit chain. It is never part of the default test or CI lanes.

## Prerequisites

1. Start IBKR Client Portal Gateway and authenticate in the browser.
2. Confirm the gateway is reachable. The default Trust endpoint is `https://localhost:5000/v1/api`.
3. Choose one resolvable stock symbol, ETF symbol, and bond identifier for the target IBKR account.
4. Choose realistic long bracket prices for each instrument. The live what-if preview uses quantity `1` and may reject prices that are too far away from the current market.

## Run

Start with the gateway readiness check:

```bash
export TRUST_IBKR_ACCOUNT_ID="DU1234567"

make ibkr-live-preflight
```

Then configure the symbols and realistic bracket prices:

```bash
export TRUST_IBKR_ACCOUNT_ID="DU1234567"
export TRUST_IBKR_STOCK_SYMBOL="AAPL"
export TRUST_IBKR_ETF_SYMBOL="SPY"
export TRUST_IBKR_BOND_SYMBOL="9128285M8"

export TRUST_IBKR_STOCK_ENTRY_PRICE="200"
export TRUST_IBKR_STOCK_STOP_PRICE="190"
export TRUST_IBKR_STOCK_TARGET_PRICE="220"
export TRUST_IBKR_ETF_ENTRY_PRICE="500"
export TRUST_IBKR_ETF_STOP_PRICE="475"
export TRUST_IBKR_ETF_TARGET_PRICE="550"
export TRUST_IBKR_BOND_ENTRY_PRICE="99"
export TRUST_IBKR_BOND_STOP_PRICE="95"
export TRUST_IBKR_BOND_TARGET_PRICE="103"
```

Check the environment before contacting the gateway:

```bash
make ibkr-live-env-check
make ibkr-live-input-check
```

Then run the full non-mutating contract-resolution and order-preview smoke:

```bash
make ibkr-live-e2e
```

## Paper order lifecycle

Only after the non-mutating suite passes, choose stock prices unlikely to execute immediately and enable the explicit mutation acknowledgement:

```bash
export TRUST_IBKR_ALLOW_PAPER_MUTATIONS="I_UNDERSTAND_PAPER_ORDERS_ARE_REAL_SIMULATIONS"
make ibkr-paper-order-e2e
```

The test refuses non-paper accounts, uses quantity `1`, performs gateway preflight and local risk validation, records a submit attempt before broker I/O, submits the bracket, verifies the submission audit chain, requests cancellation, waits for IBKR's cancellation confirmation, releases local capital only after zero-fill confirmation, and verifies the complete four-event audit chain.

If cancellation cannot be confirmed, the test fails with the Trust trade and order references. Inspect the IBKR paper account manually before retrying. Never run this target against a production account or unattended gateway.

Use these optional environment variables when the gateway is not at the default endpoint:

```bash
export TRUST_IBKR_URL="https://localhost:5000/v1/api"
export TRUST_IBKR_ALLOW_INSECURE_TLS="true"
```

The `make ibkr-live-preflight` target sets `TRUST_IBKR_LIVE_E2E=1` and verifies that the Client Portal Gateway is authenticated and can select the configured account.

The `make ibkr-live-env-check` target reports every missing required live E2E variable before any gateway call is made.

The `make ibkr-live-input-check` target validates that the configured symbols are present and that the long bracket prices are positive with `stop < entry < target`; it does not contact IBKR.

The `make ibkr-live-e2e` target first checks the environment, validates the inputs, and runs preflight, then sets `TRUST_IBKR_LIVE_E2E=1` and runs:

```bash
cargo test -p ibkr-broker --test live_gateway_smoke_test live_gateway_resolves_stock_etf_and_bond_contracts -- --ignored --nocapture --test-threads=1
cargo test -p ibkr-broker --test live_gateway_smoke_test live_gateway_previews_stock_etf_and_bond_brackets_with_whatif -- --ignored --nocapture --test-threads=1
```

When `TRUST_IBKR_LIVE_E2E=1` is set, missing required environment variables are treated as a test failure, not a skip. Without that enable flag, the ignored test prints a skip message and exits without contacting IBKR.

## Expected Evidence

A passing run proves the live gateway can resolve representative stock, ETF, and bond contracts, that Trust maps them to the expected IBKR security types, and that IBKR accepts Trust's stock/ETF/bond bracket order payloads through non-mutating what-if previews. The mocked IBKR integration tests still cover actual submission handling, broker request shape, and risk rejection without live broker I/O.

The paper mutation target additionally proves one real gateway submit/cancel path and local audit-chain verification. Paper execution behavior remains simulated by IBKR and is not evidence of production fill quality, stop behavior, latency, or market-data fidelity.

## Staged setup for the service

Use the Client Portal Gateway for this adapter, not the TWS/IB Gateway socket endpoint. Start the gateway on the operator's machine, log in with the paper username in its browser, and keep the gateway private. The service container must reach that same authenticated session using valid TLS trust. Trust checks the broker's `isPaper` response as well as account accessibility; a `DU` prefix alone does not establish paper mode. IBKR documents daily browser reauthentication and limits on automated authentication: [Gateway FAQ](https://www.interactivebrokers.com/docs/web-api/authentication/cpgw/client-portal-gateway-faq).

1. Run `make assurance-unit`, then `make assurance-protocol`. Review `make assurance-status`. These commands never contact IBKR, even if live-test opt-in variables were inherited.
2. Configure the gateway address, paper account and instrument/price variables above using current operator-selected values. The illustrative prices above are not current market quotes. Keep certificate verification enabled; configure a trusted gateway certificate instead of copying the earlier insecure-local-TLS example into a deployment.
3. Run `make ibkr-paper-preflight`. This requires current levels 1 and 2 and explicitly contacts only the non-submitting authentication, contract-resolution and what-if tests. Missing configuration or missing test evidence fails the gate.
4. Configure the deployed service with `allow_live: false`, bind the exact paper account, import the intended stock/ETF contract, reconcile its actual capital and set explicit per-trade/monthly policies. Use the owner's mandate to prepare a new long, one-unit bracket. Neither the harness nor the tests should invent a deposit to make the trade affordable. Choose an entry unlikely to execute immediately; unexpected fills make this cancel-only smoke inconclusive and require operator reconciliation.
5. Store a short-lived scoped OAuth bearer token in a private file outside source control. It needs account-scoped read, preview and trade permissions. Run the supervised MCP smoke, replacing every placeholder with the reviewed values:

```bash
python3 scripts/ibkr_paper_acceptance.py \
  --service-url https://trust.example.com \
  --token-file /private/trust-paper-token \
  --account-id TRUST_ACCOUNT_UUID \
  --broker-account-id DU_PAPER_ACCOUNT \
  --trade-id PREPARED_TRADE_UUID \
  --run-id UNIQUE_RUN_UUID \
  --max-notional REVIEWED_AMOUNT_IN_TRADE_CURRENCY \
  --acknowledge-paper-orders I_UNDERSTAND_PAPER_ORDERS_ARE_REAL_SIMULATIONS
```

The harness uses authenticated MCP, validates the exact paper binding, fresh trade, direction, asset class, one-unit legs and notional ceiling, previews, funds, submits, cancels and inspects the final state/audit. It writes a private, fsynced receipt before each mutation, uses stable keys, refuses duplicate run IDs, disallows redirects and never retries an uncertain mutation. Tokens and raw broker responses are not written to its receipt. The receipt contains operation IDs for inspection. If it stops after funding or submission, inspect the original operations, trade and broker account; do not run a new intent to bypass uncertainty. There is no account-wide cancellation or automatic liquidation cleanup.

A smoke pass explicitly leaves level 4 incomplete. Independently verify protective child activation, stop/target execution and sibling cancellation, partial fills, commissions, positions and account cash; exercise session/restart recovery and remaining operational acceptance before considering a live canary. Record an unobserved scenario as inconclusive. IBKR's [paper simulator limitations](https://www.ibkrguides.com/clientportal/aboutpapertradingaccounts.htm) prevent paper results from establishing live liquidity, fill prices or a guaranteed stop loss.
