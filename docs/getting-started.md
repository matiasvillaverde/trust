# Getting started

Trust runs as an authenticated service. MCP and the `trust` terminal client share its risk checks, broker adapters, ledger, and operation journal.

1. Follow [service deployment](service.md#running-the-service) to configure your existing OAuth/OIDC provider, account grants, persistent storage, and IBKR or Alpaca connection.
2. Connect your MCP client to the configured public URL plus `/mcp` and authenticate at your provider.
3. Read the `trust-risk` skill, then call `service_status`, `policy_get`, and `accounts_list`.
4. Use the owner identity to create a paper account profile, record its ledger capital, configure risk rules, and resolve instruments. Give each agent explicit account/scopes in the operator configuration and restart.
5. Preview size and risk before preparing, funding, or submitting a trade. Reconcile broker outcomes; accepted orders are not fills.

For a terminal session, install the default client with `cargo install --path cli --bin trust`, set `TRUST_SERVICE_URL` and `TRUST_ACCESS_TOKEN_FILE`, then run `trust status` and `trust tools`. It does not require database or broker credentials.

For development, use the pinned Rust toolchain and run `cargo test -p trust-service -- --test-threads=1`. See [architecture](architecture.md), [core concepts](core-concepts.md), and [the service operations guide](service.md) for details.
