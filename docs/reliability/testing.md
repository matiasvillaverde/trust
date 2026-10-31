# Tests that protect business behavior

A financial test should explain what money or authority it protects, exercise the production entry point, and fail when that behavior breaks. A high test count or coverage percentage does not establish this on its own.

## Choose the boundary

- Use `TrustFacade` with real, isolated SQLite for cash movement and trade accounting. Let the production validators, transactions, and projections run together. Use a broker double only when external observations or acknowledgements are needed.
- Use model/core unit tests for pure calculations and invariants, including exact Decimal boundaries. Expected results should come from worked financial examples or an independent reference model, not the production calculator being tested.
- Use authenticated HTTP/MCP tests for scopes, owner authority, idempotency, tool contracts and recovery guidance. Public serialization, tool schemas, credential redaction and dependency boundaries remain valuable contracts to test.

Put financial scenarios in the owning crate. CLI tests should cover parsing, presentation and the actual client boundary. New tests belong in a Cargo-discovered integration file or a declared test module; standalone Rust drafts are not evidence.

## Build a useful scenario

1. State the outcome: for example, moving USD 125.37 leaves USD 874.63 in the source and USD 125.37 in the reserve, with matching receipts and unchanged EUR balances.
2. For rejection, make every other precondition valid. An overdraft test between unrelated accounts can pass because the hierarchy is invalid, even if the cash limit is broken.
3. Check the side effects as well as the result: balances, lifecycle state, receipts, histories and any external calls. Rejection must preserve the relevant persisted state.
4. Pair an invalid boundary with a valid neighbor. Rejecting one cent over the available cash is useful only if transferring the exact balance succeeds.
5. For multi-write operations, fail after an earlier write and check rollback from both accounts. Remove the fault, reconnect, and verify that one retry books exactly one operation. Do not retry an ambiguous broker mutation this way; reconcile it first.
6. Exercise stale inputs, replays and process reconnection. An agent's cached `New` trade cannot fund an already funded, filled, canceled or closed trade.

Use SQL only where a storage failure must be injected. Keep behavioral expectations on public domain reads so internal SQL/query refactors do not require rewriting the oracle. Error wording is normally incidental; in fault tests an exact injected marker confirms the intended failure boundary was reached.

## Current examples

| Contract | Executable evidence |
| --- | --- |
| Transfers move existing capital, preserve currencies, and cannot overdraw | [`capital_contracts.rs`](../../core/tests/capital_contracts.rs): exact fractional amounts, valid hierarchy directions, rejection snapshots, and generated transfer sequences checked against a small independent ledger of cents. |
| A failed cash operation cannot leave a debit, partial allocation or misleading history | The same suite injects failures in receipts, balance projections and distribution history; it checks rollback, reconnection and one successful retry. |
| Allocation results, actual cash and audit receipts agree | The same suite checks all four destinations, zero/negative amounts, minimum-threshold equality, insufficient cash, fractional precision and persisted history in USD/EUR/BTC. |
| Funding consumes a reservation once | [`reliability.rs`](../../core/tests/reliability.rs): long/short funding, fills, cancellation, profitable/loss exits and replay from fresh, stale and fabricated client state. |
| A test can detect a financial regression | [`mutation_gate.py`](../../scripts/mutation_gate.py): deliberately wrong transfer credit, omitted rollback, bypassed threshold, incorrect allocation history and repeat funding, alongside existing policy/authority/stop protections. |

The generated transfer suite uses 32 sequences with up to 24 operations each. It checks acceptance against available cents, exact persisted balances, rejection side effects, currency isolation and reopening the database. Generated cases supplement explicit boundaries; they do not replace them.

Run the focused contracts:

```bash
cargo test --locked -p core --test capital_contracts --test reliability -- --test-threads=1
```

Run the curated mutation experiments after preparing the native dependency with `make native-sqlcipher`:

```bash
make assurance-mutations
```

The runner uses an isolated source copy and separate Cargo target directory. Each experiment must first pass its unmodified test, then compile and fail that exact test with the fault introduced. Compilation failures, missing tests and surviving mutations fail the gate. Results and log hashes are saved under `.local/assurance/mutations/`; the source fingerprint must remain unchanged. This is a curated set of 16 experiments, not an exhaustive mutation score.

`make assurance-unit` requires these cash and reservation scenarios in level 1 evidence. Use the workspace formatting, Clippy, all-feature, no-default-feature and coverage gates for wider changes. Preserve meaningful regression cases when consolidating tests; remove vacuous constructors and print-only probes only after their intended behavior has executable coverage.

These cash contracts exercise local accounting, not broker deposits or withdrawals. They do not establish real IBKR fills, stop execution, network recovery or power-loss durability; those require their corresponding broker and operational acceptance levels.
