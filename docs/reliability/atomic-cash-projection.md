# Atomic reuse of verified cash

The cash calculation now keeps its verified result current during ledger writes.
After one initial replay, ordinary append-only risk checks read that saved result
without fetching cash receipts again. Funding and submission each check current
cash and current rules; a previous authorization is never cached.

The durable ledger and `accounts_balances` remain the persistent financial data.
The verification record is private, connection-local SQLite TEMP state in memory.
It is deliberately rebuilt after restart. No new persistent table, schema
migration, backup format, plaintext temporary file or owner-facing setting is
introduced. This extends the [verified-prefix implementation](verified-ledger-checkpoints.md).

## What is saved and when it is trusted

For each account and currency, the record contains the exact cash total, its last
ledger ordering coordinate, the main database version, the TEMP schema version
and a pending-write marker. SQLite triggers maintain it inside the same statement
and savepoint boundaries as the receipt:

1. A BEFORE INSERT trigger invalidates replacements, historical insertions and
   incomplete earlier updates. It marks an ordinary cash append as pending.
2. An AFTER INSERT trigger folds the new receipt using the model's checked
   `Decimal` arithmetic, then clears the pending marker. The SQL function does
   not perform risk-policy decisions or floating-point arithmetic.
3. A malformed receipt or arithmetic overflow poisons the saved total. A later
   offsetting receipt cannot erase that error; a full ordered replay must succeed
   before the result can be used again.
4. Updates and deletions invalidate verification. Rollback restores both the
   receipts and verification state together. If saving the result fails, normal
   financial commands roll back their balances, receipts, orders and trade state.

A hot read executes **one prepared balance lookup plus one TEMP schema-version
check**. The lookup binds account and currency, checks the main database version
in a main-database read snapshot, and requires an intact schema epoch, a complete
write and a non-poisoned total. The independent metadata check is intentional:
SQLite's table-valued `pragma_schema_version` reports main, even when qualified
with `temp`; `PRAGMA temp.schema_version` is needed to inspect TEMP correctly.

Removing and recreating an identical trigger changes that schema epoch. A reader
therefore cannot overlook receipts written while the trigger was absent. Cold
recovery checks the actual table and trigger definitions before establishing a
new verified result. External commits invalidate the connection's main version;
they are never assumed to have run this connection's TEMP triggers.

## Evidence and regression guards

The workload uses real core rules and SQLite, with an offline broker. It creates,
funds, submits and synchronizes 4,800 long/short trades, checking exact realized
P&L and cash. Ordinary append-only cash verification decodes **one initial ledger
row** across the workload. The 9,600 later funding/exit receipts update the saved
result while they are written. This row count excludes other financial queries,
SQLite page reads and metadata checks.

The normal test suite enforces that row budget. A separate database contract
checks exact fractional cash together with the two-statement hot-read budget.
`db.cash_projection.append` exposes the number and duration of actual folds;
`db.cash_movements` continues to report reads. Write-side folds do not increment
the rows-read counter. Neither metric includes identities, amounts or SQL values.

New contracts also cover failed projection writes, rollback of a real funding
operation and a successful retry, invalid appends blocking broker submission,
ordered overflow, malformed cursor storage, identical trigger restoration,
trigger side effects, ignored projection completion, and abrupt process exit.
The crash test exits a child process without Rust destructors while both a
receipt and its derived cash are uncommitted, then checks reopened cash and
receipts. Existing external-writer, nested rollback, backup/restore, policy-change
and loss-after-funding tests remain required.

Four additional compiled mutations change debit accounting, accept an invalid
amount, bypass the schema epoch, or bypass the pending marker. The corresponding
exact-cash or no-broker-effect assertions must reject each mutant.

Evidence, source baseline and benchmark executables are retained in
`.local/cash-projection-20261103/`. The before/after profiles alternate binaries
on the same host, with one warmup and three measured runs per implementation.
No live IBKR orders or broker-connected acceptance tests are involved.

The final alternating comparison, including the pending-write guard, recorded:

| 4,800-lifecycle workload | Previous prefix replay | Atomic reuse |
| --- | ---: | ---: |
| Median cash-read time | 1.128 s | 0.454 s |
| Cash rows decoded | 9,600 | 1 |
| Median complete workload | 12.689 s | 12.479 s |

Cash reads were about 2.5 times faster. The complete workload includes the new
write costs and improved only modestly. Host load varied substantially between
runs; these samples are not a confidence interval or a promise of equivalent
live-broker gains. The SQL function itself took about 8 ms over 9,600 folds;
that excludes the surrounding trigger and storage work, which remains included
in the complete workload measurement.

```bash
make perf-profile PROFILE_TRADES=4800 PROFILE_OUTPUT=.local/cash-profile.json
make perf-gate
make service-profile
cargo test --locked -p db-sqlite checkpoints -- --test-threads=1
cargo test --locked -p core --test reliability -- --test-threads=1
python3 scripts/reliability.py --coverage
python3 scripts/mutation_gate.py
```

## Limits

Verification is valid only for the current connection and authoritative snapshot.
Cold starts, external commits, historical corrections and monitor changes may
require full replay. Read-only connections or unsupported TEMP configurations
retain full replay. A saved balance is not a broker snapshot, a risk decision or
permission to bypass reconciliation, halt, loss, stop, mandate or margin rules.

Arbitrary code that can alter private TEMP values, replace native SQL functions
or tamper with SQLite schema-version headers is inside the database trust
boundary. The verification record is not a cryptographic audit log. Existing
encryption, access controls and durable audit remain necessary. The ledger is the
recovery source; this change does not claim constant-time restart or repair.
