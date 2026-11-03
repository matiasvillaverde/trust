# Test contract improvements — 2026-11-01

This change addresses the **112 low-value candidates and six orphan functions** in the [2026-10-31 audit](test-usefulness.md). The 52 unchecked CLI rendering/routing tests are included in the 112; they are not an additional 52. The [replacement register](test-usefulness-replacements.csv) maps every original identity to its current executable contract and explains the replacement. Several originals share a stronger scenario: 118 original entries map to 103 distinct replacement tests, including strengthened existing tests.

The original catalog remains a historical snapshot with source links pinned to its commit. We have not assigned new subjective scores or claimed an exhaustive review of the other tests.

## What the replacements protect

| Original category | Entries | Observable contract |
| --- | ---: | --- |
| Unchecked output/routing | 52 | Capture actual terminal output or run the real CLI; verify identifiers, amounts, status, error causes, account scope, pagination, timestamps and zen/secret handling. Financial workflows also check persisted receipts and balances. |
| Panic-based paths | 33 | Missing input returns an actionable error. Incomplete commands exit with usage before opening a database. Empty interactive selections leave capital and receipts unchanged; unexpected panics fail. |
| Incidental/debug/test-double checks | 16 | Exercise production workflows with doubles only at external boundaries; read back complete persisted records and verify exact allocations, stop protection, replay rejection and terminal reconciliation. |
| Permissive outcome checks | 10 | Require the intended success or specific rejection, reach the intended broker failure, and compare state before and after. |
| Presence-only tail-loss calculation | 1 | Independently calculated Decimal tail averages for long/short trades, confidence boundaries, input permutations, scaling and missing risk evidence. |
| Unregistered formatter tests | 6 | Cover their useful behavior through active rendering and process contracts; remove the unused `cli/src/output.rs` module. |

Some tests remain small unit tests deliberately. Decimal formulas, input validation and renderer behavior have useful contracts without creating an entire service. Integration scenarios use real SQLite and the facade; executable tests cover parser, dispatcher and output together. We do not assert that every replacement is an end-to-end broker test.

Examples with independent financial expectations:

- A 10-unit fill at $100 with a $1.25 fee reserves/deploys $1,000, charges exactly $1.25, returns the persisted filled order, and cannot be applied twice.
- Manual exits at $94.50 and $110.25 produce the exact proceeds, fees and realized losses/profits, with no remaining market capital or duplicate receipts on replay.
- Canceling before submission refunds $20,000 once without calling the broker. A failed cancellation after submission reaches the broker exactly once, preserves the reservation and records an unresolved audit outcome.
- Cash reporting reconciles a $10,000 deposit, $4 net realized profit, $100 deployed, a $12.37 withdrawal and a $3.19 deposit to **$9,894.82**. Daily, weekly and monthly buckets sum to that same amount.
- Distribution of $12.37 at 50/30/20/0 percent preserves **$6.185 + $3.711 + $2.474 = $12.37**, without rounding away reserves.
- Loss observations of −4R, −2R, +1R and +3R have a worst-half average of **−3R**, an all-observation average of **−0.5R**, and worst-observation loss of **−4R**.

The output capture helper runs one exact test in a subprocess, requires completion markers and successful exit, and bounds execution time. Selecting zero tests, panicking, or failing to finish cannot satisfy the parent assertion. It uses an isolated database path, disables protected-keyword keychain lookup and turns off the live-IBKR test flag. Trading scenarios use controlled broker doubles; these environment flags are not a general network or credential-store sandbox.

IBKR connection-setting tests inject a deterministic credential store underneath the production validation, serialization and account/environment selection. They check isolation, failed writes, corruption and deletion without silently skipping when a desktop keychain is unavailable. Native OS credential-store compatibility remains a separate acceptance concern.

## Product defects exposed and fixed

- Manual fills persisted execution details but returned a stale entry order. The returned trade now includes the persisted filled quantity, status and timestamp.
- The legacy cash timeline ignored funding and repayments. It now uses the available-cash movements supplied by the ledger, including fees and withdrawals, without counting order fills twice.
- Legacy rule commands queried a Clap argument absent from their command definition and panicked. They now use their existing text-output contract.
- `level rules` without a nested command reached an unreachable branch. Clap now rejects it with usage before database access.
- Several dialog builders panicked on missing input, and selection helpers hid I/O failures as cancellation. They now preserve actionable errors. Watch setup preserves the original selection failure.

These changes do not change risk-policy authority, broker submission rules or the MCP schema. Four large dialog test modules were split out under `cfg(test)`. Six duplicated broker doubles were consolidated into one configurable test fixture; unconfigured mutations return errors and assertions remain in product workflows. No architecture size allowance was increased, coverage threshold lowered, or new coverage exclusion added.

## Verification

Validated on 2026-11-01. All 118 register entries resolve to source tests; all 103 distinct replacement tests appear as passing in the complete all-features execution log.

| Check | Result |
| --- | --- |
| All-features workspace, with LLVM coverage | **2,469 passed, 0 failed, 9 ignored**; all required reliability regressions present. |
| No-default-features workspace | **1,849 passed, 0 failed, 8 ignored**. |
| Curated mutation gate | **24/24 detected**; every unmodified baseline passed, every compiled mutant failed its named test, source unchanged during the experiments. |
| Line / function / region coverage | **94.9793% / 90.0540% / 93.5179%**; unchanged minimums 94% / 90% / 93%. |
| Formatting, all-target/all-feature Clippy, architecture and diff checks | Passed. Architecture allowances reduced from 47 to 44. |
| Python script contracts | 39 passed. |
| CLI JSON snapshots | Passed within the all-features workspace run. |
| Structured source review | `autoreview --mode local`, two complete bounded passes, no accepted/actionable blocking findings. The previous historical catalog was supplied as baseline context in an isolated review copy. |

Local evidence is under `.local/test-value-20261101/`: `validated.json` and `validated.log`, `coverage.json`, `no-default.json`, `mutation-results.json`, and `review-final.json`. The mutation logs and their hashes are also under `.local/assurance/mutations/`. The initial coverage runs correctly failed the function threshold after removal of mock self-tests; duplicate broker and unused confirmation stubs were consolidated before the final passing run. No coverage exclusion or threshold was relaxed.

Reproduce the checks:

```bash
cargo fmt --all -- --check
python3 scripts/check_architecture.py --base HEAD
cargo clippy --workspace --all-targets --all-features -- -D warnings
python3 scripts/reliability.py --output .local/test-value-20261101/workspace-rerun.json
cargo test --locked --no-default-features --workspace -- --test-threads=1
python3 scripts/reliability.py --coverage --output .local/test-value-20261101/validated.json
python3 scripts/mutation_gate.py
python3 -m unittest discover -s scripts -p 'test_*.py'
```

The mutation gate adds five deliberate compiled faults: wrong tail averaging, stale fill evidence, ignored funding in the timeline, wrong displayed available cash, and shared IBKR account settings. Each named baseline must pass and the corresponding mutated test must fail; compilation errors and zero-test runs do not count as detection. These selected experiments are not an exhaustive mutation score.

Ignored broker tests are not run. These results establish offline behavior against controlled boundaries; they do not establish actual IBKR fills, native credential-store compatibility or immunity to trading losses. The remaining mixed/indexed catalog entries and real broker acceptance remain separate work.
