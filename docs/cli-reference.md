# Trust terminal client

`trust` is a client of the authenticated Trust service. MCP is the primary agent interface; both clients use the same catalog, permissions, and operation journal.

```text
trust [--url ORIGIN] [--token-file FILE] status
trust [--url ORIGIN] [--token-file FILE] tools
trust [--url ORIGIN] [--token-file FILE] skill NAME
trust [--url ORIGIN] [--token-file FILE] call TOOL [--input JSON_FILE]
```

`TRUST_SERVICE_URL` and `TRUST_ACCESS_TOKEN_FILE` provide the defaults. Without `--input`, `call` reads a JSON object from stdin. Tokens are read from a file and never passed as command-line values. Use HTTPS except for loopback development. Results are JSON, and failed operations produce a nonzero exit code.

Discover exact schemas with `trust tools`. All mutations require an explicit stable `idempotency_key`. On timeout use `operation_get` with the original key; never blindly repeat under a new key.

See [service setup, tools, and skills](service.md) for the complete workflow. The previous terminal application is available as `trust-legacy` only when built with `cargo build -p trust-cli --features legacy-cli --bin trust-legacy`; its [reference](legacy-cli-reference.md) is retained for migration and regression coverage. Never use it on a service-owned database.
