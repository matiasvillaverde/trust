# Financial admission profiling

The latest optimization is described in [verified ledger checkpoints](verified-ledger-checkpoints.md).
The measurements below record the earlier profiling and optimization passes that
identified repeated cash replay as the principal scaling problem.

The real, risk-enabled 1,200-lifecycle gate originally took 17.764 seconds against
its unchanged six-second budget. Instrumentation identified repeated ledger reads
and row decoding as the largest cost. The profile includes actual draft creation,
funding, submission, reconciliation and persisted cash checks through `TrustFacade`
and SQLite, with a controlled broker adapter.

## What the measurements showed

Single instrumented runs on the same local machine, with 1,200 lifecycles each:

| Measurement | Before | After |
| --- | ---: | ---: |
| Workload wall time | 15.699 s | 4.422 s |
| SQL loading, exclusive time | 11.082 s | 1.875 s |
| Monthly reference, inclusive time | 9.450 s | 0.640 s |
| Monthly-reference rows fetched | 8,637,600 | 2,400 |
| Cash-ledger reads | 6,000 | 2,400 |
| Cash-ledger rows fetched | 7,202,400 | 2,881,200 |

These are diagnostic samples, not benchmark medians. The before/after reports are
retained locally as `before-1200.json` and `reused-validation-1200.json` under
`.local/performance-instrumentation-20261101/`. Row counts mean rows returned to
Rust, including repeated reads; they are not SQLite page reads or unique records.

The scoped subscriber reports wall time both including nested spans and excluding
them. Use exclusive time to rank costs without double-counting. Instrumentation
adds overhead, and SQL loading includes Diesel row construction; this is not a
CPU sampling profile. An uninstrumented repeated gate provides the acceptance
measurement separately.

Release gates passed at 3.852 seconds and, after preserving the typed rejection
for invalid stop geometry, **4.792 seconds median** for 1,200 lifecycles. The latest
run is approximately **3.7× faster** than the earlier 17.764-second median.
Analytics took 50.2 ms in the latest run. Each gate uses three measurements after
one warmup and the original 6,000/1,000 ms budgets. Both runs are retained as
`performance-gate.log` and `performance-gate-final.log` in the same local evidence
directory; timings vary between runs. Compiler optimization, fixture sizes, risk
policies and financial assertions were retained. The full-suite geometry contracts
caught an error-classification regression during the refactor; their expectations
were preserved and the validation path was corrected before the final gate.

With production financial telemetry enabled, the encrypted real-process service
gate also passed. HTTP p95 was 2.46 ms sequentially and 9.10 ms at concurrency
four; MCP p95 was 2.95 ms and 9.58 ms respectively. These loopback fixtures retain
JWT verification and durable read auditing, but have small seeded histories and
controlled broker responses. They are local regression measurements, not live IBKR
latency or capacity claims. See `service-performance-gate.log` for all samples.

## Changes and financial invariants

SQLite now selects only the inputs needed for the monthly reference: earlier
funding/refund rows, current capital flows, and closing receipts for earlier
reservations. All query branches bind account, currency and cutoff and exclude
soft-deleted rows. Core still decides how those receipts affect capital using
checked `Decimal` arithmetic. Other database adapters retain a safe full-history
default for the new narrow read capability.

`EXPLAIN QUERY PLAN` also exposed a broad chronological scan caused by ordering
the SQL union. Sorting the smaller selected result after indexed filtering avoids
that scan while preserving deterministic receipt ordering.

Funding resolves cash, risk rules and base sizing once, then applies the current
level cap. Submission independently reloads the ledger, current policy, reservation
and level. It passes raw verified cash into the monthly calculation so its own
reservation is restored exactly once. That first pass did not reuse cash across
requests. The subsequent checkpoint implementation reuses a verified ledger
prefix with transactional invalidation, rather than the account balance projection. Risk
thresholds, broker behavior and transactional writes remain unchanged.

At that stage, cash-ledger replay still grew with history. Removing duplicate
replay did not make lifetime-history admission constant time.
Older carried reservations also require historical checks. Future optimization
must preserve corruption detection and exact financial results.

## Reproduce

Use the pinned Rust toolchain and prepare the native dependency with
`make native-sqlcipher`. The following commands use controlled offline brokers:

```bash
make perf-profile PROFILE_TRADES=1200 \
  PROFILE_OUTPUT=.local/performance-profile.json
make perf-gate
make service-perf
make service-profile SERVICE_PROFILE_OUTPUT=.local/service-profile.json
make ledger-profile LEDGER_PROFILE_OUTPUT=.local/ledger-profile.json
TRUST_PROFILE_SQL_PLAN=1 cargo test --locked -p db-sqlite --lib \
  workers::worker_transaction::tests::ledger -- --nocapture --test-threads=1
```

`perf-profile` enables a subscriber only for this workload. It accepts static span
names and the numeric `rows` field, rejects additional field names, and never
captures event bodies or financial arguments. Its JSON report is sorted by
exclusive time. The normal test suite runs a small instance of the same workload
to verify cash conservation, timing aggregation and payload restrictions.

`perf-gate` uses three measurements after a warmup and retains the default budgets:
1,000 ms for 36,865 analytical records and 6,000 ms for 1,200 persisted lifecycles.
Run performance measurements without concurrent builds or coverage collection.
The lifecycle fixture uses in-memory SQLite, so it does not measure encrypted disk
durability, broker-network latency or production capacity. `service-perf` exercises
encrypted storage and real authenticated HTTP/MCP processes with audit enabled;
see the [service measurement conventions](../service.md).

## Operational telemetry and regression protection

The service collects only an allowlist of 15 financial stages into authenticated
Prometheus metrics:

- `trust_financial_stage_duration_seconds{stage}`: inclusive stage duration.
- `trust_financial_rows_read_total{stage}`: fetched ledger rows.

Parent and child duration sums overlap. Compare stages individually rather than
adding them. The allowlist includes cash verification, monthly reference,
protected capital, quantity calculation, ledger loading and decoding. No account,
symbol, trade, SQL parameter, token or request value becomes a label or log field.
Production collection uses a separate tracing layer; the existing operational-log
filter still suppresses provider/core message payloads.

Contracts verify exact required receipts, account/currency isolation, soft deletion,
corrupt amount rejection, long/short level caps and negative ledger rejection despite
a positive cached projection. Real-process HTTP/MCP tests verify that actual trade
work emits nonzero metrics while retaining redaction, replay and audit checks.
The mutation gate deliberately removes query account scoping and double-counts a
submission reservation; each experiment must compile and fail its financial contract.

The first 33-experiment run exposed a gap in the reservation test: changing policy
alone still rejected submission when reserved cash was counted twice. The
2026-11-03 follow-up exercises a realized loss after funding instead. For both
longs and shorts, a $300 loss leaves enough of the original $500 monthly budget
to submit a pending $20-risk bracket; a $495 loss leaves only $5 and must reject
with `RiskPerMonthExceeded`, unchanged financial tables and no broker submission.
Cash and the current level independently permit the order in both cases. The
faulty double-counting implementation now fails this contract. Submission cases
live together in `core/tests/reliability/submission.rs`, with their names required
by the reliability and assurance runners.

Run the wider financial gates after changing these paths:

```bash
cargo fmt --all -- --check
python3 scripts/check_architecture.py --base HEAD
cargo clippy --workspace --all-targets --all-features -- -D warnings
python3 scripts/reliability.py --coverage \
  --output .local/performance-instrumentation-20261101/validated.json
cargo test --locked --no-default-features --workspace -- --test-threads=1
python3 scripts/mutation_gate.py
```

The offline reliability report retains individual outcomes and a source fingerprint;
coverage keeps the existing 94% line, 90% function and 93% region thresholds. The
curated mutation register contains 33 experiments. None of these commands enable
ignored live-broker tests.

## Follow-up: history growth and service queueing (2026-11-03)

The next profile used the optimized implementation above, with a fresh local run
at each history size. These are individual instrumented samples, not medians:

| Measurement | 1,200 lifecycles | 4,800 lifecycles |
| --- | ---: | ---: |
| Workload wall time | 4.148 s | 36.818 s |
| Cash-ledger reads | 2,400 | 9,600 |
| Cash-ledger rows returned, including repeat reads | 2,881,200 | 46,084,800 |
| Cash-ledger read, inclusive time | 1.447 s | 22.813 s |
| SQL loading across ledger queries, exclusive time | 1.791 s | 23.476 s |
| Cash-row decoding, exclusive time | 0.241 s | 4.441 s |
| Monthly-reference input query, inclusive time | 0.510 s | 4.781 s |

Four times the lifecycles require roughly sixteen times the cash rows and 8.9 times
the total time. Full cash-ledger replay is the main scaling bottleneck: its read
span occupies 62% of the 4,800-lifecycle workload. The SQL-loading row includes
work inside both cash and monthly queries; do not add it to their inclusive totals.
This confirms that removing duplicate reads in the earlier pass improved the
constant cost but did not remove quadratic accumulated work as history grows.

The new service profile samples authenticated `/metrics` immediately before and
after each isolated batch of `risk_preview` requests. It records request percentiles,
queue wait, authentication, authorization, domain execution, audit completion and
the 15 financial stages. It preserves encryption, JWT verification, full synchronous
audit writes and the single financial worker. Every response must give exactly
20 units for the fixture's $100 loss budget and $5 stop distance; every measured
and warm-up read must appear in the verified audit chain, with no broker submission.

The first service sample found:

| Stage, mean per call | HTTP, sequential | HTTP, concurrency 4 | MCP, concurrency 4 |
| --- | ---: | ---: | ---: |
| HTTP route duration | 2.503 ms | 6.422 ms | 5.936 ms |
| Financial-worker queue wait | 0.015 ms | 3.791 ms | 3.463 ms |
| Domain execution | 1.576 ms | 1.657 ms | 1.519 ms |
| Monthly-reference input query | 0.680 ms | 0.717 ms | 0.659 ms |
| Authorization | 0.374 ms | 0.398 ms | 0.354 ms |
| Audit completion | 0.089 ms | 0.091 ms | 0.088 ms |
| Authentication, all routes | 0.031 ms | 0.035 ms | 0.035 ms |

Queueing occupies about 59% of mean route time at concurrency four. Reducing work
inside the financial worker should therefore improve both execution and waiting
time. Authentication and audit completion are small in this fixture. The monthly
query still costs about 0.7 ms with only one returned row. Its raw SQL is prepared
on every call by Diesel 2.3.14; the current spans combine preparation, execution
and row construction, so this profile does **not** attribute all that time to
preparation. Parent and child stage times overlap. Authentication includes the
closing metrics scrape; the other recorded stages cover only the isolated batch.

`ledger-profile` isolates the actual cash-read worker on an in-memory database
with 24,001 synthetic receipts, of which 14,401 affect cash. It verifies the
independent $1,048,000 closing cash expectation and identical ordered inputs for
all index variants. After a warmup, eleven reads per variant produced a median
6.572 ms with the current index and 5.355 ms with an experimental covering index
on `(account_id, currency, created_at, id, category, amount, trade_id)`. The query
plan explains the difference: the current chronological index selects an account;
the candidate also selects currency and contains all projected values. A second
candidate placing category before date was not chosen by SQLite.

These indexes are created only in the disposable diagnostic database. The 19%
read-time reduction does not establish a net benefit after index writes, storage
growth and encrypted durability, and does not remove full-history replay. No schema
or financial algorithm changed in this follow-up. A monthly-query experiment was
also discarded because its plan would rescan recent closing categories for each
carried trade; it was not a safe improvement across history shapes.

These results motivated the [checkpoint implementation and its correctness
contracts](verified-ledger-checkpoints.md). It verifies edits, corruption, rollback,
restarts and account isolation while retaining one financial worker. Monthly
query changes retain explicit carried-trade and snapshot contracts.

Evidence is retained locally under `.local/performance-depth-20261103/`:
`before-1200.json`, `before-4800.json`, `service-before.json`,
`cash-index-experiment.log`, and the monthly-query plans. These are offline,
single-machine measurements. The service fixture has a small history; combine
its stage report with the separate lifecycle scaling report rather than treating
it as large-account service capacity or live IBKR performance.

To reproduce the growth comparison, run the two `perf-profile` sizes sequentially
with no competing builds. The new profile commands write JSON, fail on financial
or audit assertions, and keep the existing service latency budgets:

```bash
make perf-profile PROFILE_TRADES=1200 PROFILE_OUTPUT=.local/history-1200.json
make perf-profile PROFILE_TRADES=4800 PROFILE_OUTPUT=.local/history-4800.json
make service-profile
make ledger-profile
```
