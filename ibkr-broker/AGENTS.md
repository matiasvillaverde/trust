# Interactive Brokers guide

This file extends the repository `AGENTS.md` for `ibkr-broker/`.

`IbkrBroker` implements `model::Broker` over the synchronous Client Portal Gateway API.

- `client.rs` and `config.rs`: session, HTTP, gateway, and keychain configuration.
- `contracts.rs`: contract search/resolution and metadata.
- `orders.rs`: bracket payloads, reply handling, live-order lookup, mapping, and validation.
- `market_data.rs`, `executions.rs`: broker data conversion.
- `support.rs`, `parsing.rs`: account checks and strict parsing helpers.

## Safety contracts

- Validate trade/account identity, account broker kind, supported asset class, symbols, price relationships, and required order references before remote mutation.
- Preserve Client Portal session preparation and broker reply-confirmation handling. IBKR may return confirmation prompts instead of a final order result.
- Treat submit, cancel, close, and modify as externally mutating and non-transactional. Check idempotency and partial-failure behavior before adding retries.
- Keep broker account IDs, tokens, gateway data, keychain values, and raw sensitive responses out of logs and fixtures.
- Parse prices, quantities, fees, and executions into `Decimal`; do not use floats for finance.
- Keep contract resolution asset-aware. Stock, ETF, bond, crypto, and fiat behavior must be explicit rather than guessed from symbols.

## Tests

```bash
cargo test -p ibkr-broker
cargo test -p ibkr-broker --test http_integration -- --test-threads=1
```

The ignored `live_gateway_smoke_test` cases require explicit environment variables and a real authenticated gateway. Never run them without an explicit user request. Follow `docs/interactive-brokers-e2e.md` and the `make ibkr-live-*` preflight targets when authorized.
