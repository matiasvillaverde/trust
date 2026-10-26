# Trust

Trust is a risk-management service for agent-assisted trading. It sizes positions, enforces capital and risk limits, manages bracket trades, and maintains an auditable ledger. Interactive Brokers and Alpaca remain behind broker adapters; Trust owns the risk decisions.

Agents connect through authenticated MCP. The `trust` CLI is a thin service client. A single financial worker serializes operations, records durable intent before mutation, and preserves uncertain outcomes for reconciliation.

```text
MCP agents / trust CLI
          ↓
OAuth access-token validation + account/scope permissions
          ↓
Durable operation journal → TrustFacade → financial ledger / broker adapters
```

Start with [service setup](docs/service.md), [getting started](docs/getting-started.md), and the [terminal client reference](docs/cli-reference.md).

- **Risk first:** exact decimal calculations, position sizing, monthly limits, leveling, and protected stops.
- **Broker workflow:** prepare → preview → fund → submit → reconcile. Submission does not imply a fill.
- **Agent access:** existing OAuth/OIDC provider, explicit subject/account/scope grants, live mutations disabled by default.
- **Operations:** nonroot container, private persistent storage, structured logs, durable idempotency records, and verifiable audit chains.
- **Tools and skills:** trading, policy, capital, reporting, journals, sessions, advisory analysis, market data, and administration. Workflow skills are bundled under [`skills/`](skills/) and served as MCP resources/prompts.

Build locally:

```sh
cargo build --locked -p trust-service -p trust-cli
./target/debug/trust-service /absolute/path/config.local.json
```

Or configure the existing identity provider and use [`deploy/compose.yaml`](deploy/compose.yaml). The [operator guide](docs/service.md) covers credentials, TLS, IBKR Gateway sessions, backup/recovery, and limits. Trust is beta software; validate behavior in paper accounts before authorizing live operations.

The Rust workspace keeps business rules in `core::TrustFacade`, domain contracts in `model`, and broker/database translations in adapters. See [architecture](docs/architecture.md) and [core concepts](docs/core-concepts.md). The historical terminal application remains behind the `legacy-cli` feature as `trust-legacy`, with its regression suite; it must not share a running service's database.

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --locked --workspace --all-features -- --test-threads=1
```

No live-broker tests run by default. See [IBKR testing](docs/interactive-brokers-e2e.md) for the explicit operator workflow. Licensed under GPL-3.0-only.
