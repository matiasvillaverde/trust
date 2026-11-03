# Verified ledger checkpoints

This records the prefix-replay optimization. The subsequent
[atomic cash projection](atomic-cash-projection.md) updates the verified result
during appends and reduces normal reads to one balance lookup plus a schema guard.
The benchmark results below describe the earlier implementation.

Repeated cash replay was the main admission bottleneck. The 4,800-lifecycle
profile reread 46,084,800 cash rows and spent 22.813 seconds inside that read.
Trust now reuses an unchanged, verified prefix and checks every new receipt with
the same ordered `Decimal` arithmetic. Financial mutations still run on one worker.

## Measured improvement

Final instrumented samples compared with the earlier profile on the same machine:

| Measurement | Previous implementation | Checkpoints and prepared monthly reads |
| --- | ---: | ---: |
| 1,200 complete lifecycles | 4.148 s | 2.544 s |
| 4,800 complete lifecycles | 36.818 s | 10.002 s |
| Cash rows decoded across 4,800 lifecycles | 46,084,800 | 9,600 |
| Cash verification across 4,800 lifecycles | 22.813 s | 0.930 s |
| Monthly-reference input reads across 4,800 lifecycles | 4.781 s | 0.167 s |

The larger workload is about 3.7 times faster. Increasing lifecycles fourfold now
increases time approximately fourfold in this fixture. These individual diagnostic
samples include instrumentation overhead. The release gates separately report
medians of three runs after a warmup: **2.339 seconds** for 1,200 lifecycles and
**9.707 seconds** for 4,800. Both pass, with the existing 6-second limit for 1,200
lifecycles and a new 16-second limit for 4,800. The normal test suite also limits
cash rows decoded while running 64 real lifecycles, so quadratic replay cannot
silently return even on a fast machine.

The lifecycle workload uses real core rules and SQLite with an offline broker.
Encrypted HTTP/MCP profiling retains authentication, durable audit writes and
financial assertions. Its small seeded history measures fixed request overhead;
the larger lifecycle result is not a claim of equivalent live IBKR latency.

Evidence is retained in `.local/ledger-speed-20261103/`, including the source
baseline, intermediate and final profiles, repeated release gates, financial
tests, mutation results and scoped review. Reproduce with:

```bash
make native-sqlcipher
make perf-profile PROFILE_TRADES=1200 PROFILE_OUTPUT=.local/ledger-1200.json
make perf-profile PROFILE_TRADES=4800 PROFILE_OUTPUT=.local/ledger-4800.json
make perf-gate
make service-profile
```

Run timing measurements without competing builds or coverage collection. Do not
raise the budgets to accept a regression.

## Checkpoint ownership and validity

`ReadTransactionDB::verified_cash_balance` is an optional optimized capability
with a complete-replay default. The shared signed cash arithmetic lives in
`TransactionCategory::apply_to_available_cash`; core still rejects a negative
final balance and enforces all funding, monthly, level and portfolio policies.

SQLite keeps the prefix total, its last `(created_at, id)` coordinate and a
connection-local database version in a TEMP table, keyed by account and currency.
The table is available only when temporary storage is in memory and the connection
allows TEMP writes. Otherwise, the adapter uses complete replay. No new persistent
table, balance projection, schema migration or backup payload is introduced.

Each read establishes a main-database snapshot inside a nested savepoint, verifies
the actual TEMP table/trigger definitions, then compares that connection's
[`data_version`](https://www.sqlite.org/pragma.html#pragma_data_version). A different
version discards the usable prefix for that scope and requires complete replay.
This handles commits from other connections and processes, whose writes cannot
run this connection's TEMP triggers. The WAL test commits an external update
while a reader snapshot is open and checks both the old snapshot and the next one.

Same-connection writes use TEMP triggers on `main.transactions`:

| Ledger change | Checkpoint behavior |
| --- | --- |
| Strictly later insertion | Verify and fold the new suffix on the next read |
| Backdated insertion or earlier ID at the same timestamp | Invalidate the affected prefix |
| Replacement of an existing ID or SQLite rowid | Invalidate before replacement, even with recursive delete triggers disabled |
| Update, soft deletion, hard deletion or scope movement | Invalidate all prefixes conservatively |
| Savepoint rollback | SQLite rolls back ledger changes and checkpoint changes together |
| Missing or changed monitor definitions | Rebuild empty TEMP state and replay the ledger |
| Restart or a new connection | Start without any checkpoint |

The suffix query uses a row-value comparison on `(created_at, id)` so the existing
chronological index can seek past the verified prefix. Cursor decoding requires
text with valid UTF-8: coercing a BLOB or replacing invalid bytes could change
SQLite ordering and repeat or skip receipts. Monetary values still use strict
decimal parsing; trade references still require valid UUIDs and categories.
Overflow remains an error in original receipt order, including when a newly
backdated deposit would overflow before a later withdrawal.

The checkpoint is private derived state, not an authorization or tamper-proof
storage boundary. Operators/processes that can arbitrarily modify the connection,
its TEMP tables or database files remain inside the database trust boundary.
Existing storage encryption, access restrictions and audit controls still matter.
Using [`temp_store=MEMORY`](https://www.sqlite.org/pragma.html#pragma_temp_store)
keeps these derived financial values out of plaintext SQLite temporary files;
builds that force file-backed TEMP storage use full replay instead.

## Monthly reads and observability

Monthly-reference queries now use fixed Diesel query shapes with bound parameters,
which permit prepared-statement reuse. Current closing receipts are fetched only
when earlier reservation movements exist. Every branch runs in one snapshot;
an integration test commits an updated ledger between branches and
requires an internally consistent result. Account/currency scope, soft deletion,
cutoff semantics, carried trades and deterministic ordering are retained.

The existing financial spans and Prometheus metrics continue to report actual
rows fetched and decoded. A warm unchanged cash read can therefore report zero
ledger rows while still validating its snapshot and monitoring definitions. These
are not SQLite page-read counters. No account IDs, values or SQL parameters are
added to production telemetry.

## Verification and limits

New database contracts cover exact fractional cash and fees, scope isolation,
backdated/tied ordering, historical edits, corrupt amounts and trade references,
invalid cursor storage, replacements, nested rollback, restart, external commits,
missing monitors, read-only fallback and monthly snapshot consistency. The
reliability runners require these tests. Four new compiled mutation experiments
remove external-version validation, edit invalidation, backdate invalidation and
replacement detection; the corresponding financial assertions must fail.

```bash
python3 scripts/reliability.py --coverage --output .local/ledger-speed-20261103/reliability.json
cargo test --locked --no-default-features --workspace -- --test-threads=1
python3 scripts/mutation_gate.py
```

Cold reads, external commits and historical edits still require a complete cash
replay. The checkpoint has a fixed validation cost, which can exceed a full read
for tiny ledgers. Monthly opening/carry calculations can still grow with prior-month
history; the current-month lifecycle profile does not establish constant-time
admission for every history shape. No ignored live-broker tests are part of this
verification, and these results do not replace IBKR paper acceptance evidence.
