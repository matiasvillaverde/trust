# Interactive Brokers E2E Smoke Test

The live IBKR smoke test verifies that an authenticated Client Portal Gateway can resolve Trust securities across the supported IBKR categories and preview Trust bracket orders with IBKR's non-mutating `/whatif` endpoint:

- stock -> `STK`
- ETF -> `STK`
- bond -> `BOND`

This test does not place orders. It is ignored by default and must be enabled explicitly because it depends on a live authenticated gateway session. The what-if preview calls IBKR's documented [Preview Order / WhatIf Order](https://www.interactivebrokers.eu/campus/ibkr-api-page/cpapi-v1/#preview-order-whatif-order) endpoint, which returns commission and margin impact without submitting the order.

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
