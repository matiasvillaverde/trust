# CLI crate guide

This file extends the repository `AGENTS.md` for `cli/`.

## Layers

- `src/main.rs`, `src/commands.rs`, `src/commands/`: Clap tree and command builders.
- `src/command_routing.rs`: typed argument extraction/routing helpers.
- `src/dispatcher.rs`: central orchestration; `ArgDispatcher` owns `TrustFacade`.
- `src/dialogs/`: interactive input only.
- `src/views/`: terminal formatting only.
- `src/exporters.rs` and dispatcher report paths: machine-readable output.

Adding a command usually requires the builder, routing, dispatcher branch, interactive/non-interactive behavior, output, and tests. Keep monetary/risk logic in core.

## Boundaries and safety

- Business mutations go through `TrustFacade`. `tests/architecture_guard.rs` enforces narrow exceptions for SQLite construction, credential management, and read-only broker metadata.
- Critical mutations must use the protected-keyword flow, support `--confirm-protected`, and return consistent text/JSON errors.
- Preserve deterministic non-interactive flags. Supplied arguments must not unexpectedly open a dialog.
- JSON output is a public contract: keep stdout machine-clean, field/error names stable, and Decimal serialization deliberate. `--zen` must suppress absolute monetary output where promised.
- Tests invoking the binary must set a temporary `TRUST_DB_URL` and normally `TRUST_DISABLE_KEYCHAIN=1`; never touch the user's database or keychain.
- Default DB paths differ for debug and release. Do not hard-code either into feature code or tests.
- Presentation-only float conversion must remain at the presentation boundary; do not use it for financial decisions.

## Tests and snapshots

CLI integration tests share database, process, and environment state; serialize them.

```bash
cargo test -p trust-cli -- --test-threads=1
cargo test -p trust-cli --test integration_test_trade -- test_name --test-threads=1
cargo test -p trust-cli --test architecture_guard
make ci-snapshots
```

Refresh `tests/snapshots/*.json` with `make snapshots-update` only for an intentional JSON change, then inspect and verify the snapshot diff.
