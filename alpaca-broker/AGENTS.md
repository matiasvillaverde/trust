# Alpaca broker guide

This file extends the repository `AGENTS.md` for `alpaca-broker/`.

`AlpacaBroker` is a synchronous `model::Broker` adapter. Operations are split by behavior (`submit_trade`, `sync_trade`, `close_trade`, `cancel_trade`, `modify_*`, market data, executions, and fees) and bridge to async `apca` calls internally.

## Safety contracts

- Validate trade/account ownership, supported asset category, and required broker IDs before keychain or network I/O. Stock, ETF, and crypto are supported; bond and fiat rejection remains typed.
- Preserve credential redaction in `Keys` debug/display output. Never log secrets or credential-bearing endpoints.
- Preserve `Decimal` end to end. Convert broker numeric representations through strings, not floats.
- Submission builds a bracket order and identifies returned legs by price. Exact representation and equal-price ambiguity make this mapping safety-critical; changes require focused tests.
- Close is a non-transactional external two-step operation (cancel target, submit market close). Cancel and modify calls also mutate remote state. Analyze idempotency and partial failure before changing ordering or retry behavior.
- Sync maps only changed orders and derives aggregate status from mapped results. Do not silently broaden partial mapping/error suppression.
- Symbol normalization differs across submission, sync, and activity feeds, especially for crypto pairs. Test equities and crypto whenever changing symbol logic.
- Execution/fee pagination is capped. Fee results may include account-level activities and are not necessarily trade-exclusive; preserve downstream deduplication identifiers.

```bash
cargo test -p alpaca-broker
cargo clippy -p alpaca-broker --all-targets --all-features -- -D warnings
```

Prefer pure request/mapping and pre-I/O validation tests. Do not require live credentials for the normal suite.
