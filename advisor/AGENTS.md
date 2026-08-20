# Advisor crate guide

This file extends the repository `AGENTS.md` for `advisor/`.

`advisor` owns external advisory configuration and analysis primitives, not trade authorization. Catalyst scans use calendar APIs; correlation and regime analysis consume broker bars. Core decides how advisory results affect user workflows.

- `config.rs`: keychain-backed, redacted provider configuration.
- `catalyst.rs`: external calendar request/response normalization.
- `correlation.rs`: correlation and position-heat analysis.
- `regime.rs`: breadth, trend, volatility, and composite regime analysis.
- `error.rs`: public typed failures.

## Rules

- Never expose API keys through debug output, errors, fixtures, or logs. Keep configuration status redacted.
- Keep HTTP/provider payloads at the boundary and return stable advisor-domain types.
- Use `Decimal` and checked arithmetic for thresholds, returns, heat, and derived values. Handle missing/short/misaligned bar series explicitly.
- Advisory failures and insufficient data must not silently become approval. Return an explicit error or advisory status and let core apply policy.
- Keep unit tests deterministic; mock provider payloads and fixed bar series rather than relying on live services.

```bash
cargo test -p advisor
cargo clippy -p advisor --all-targets --all-features -- -D warnings
```
