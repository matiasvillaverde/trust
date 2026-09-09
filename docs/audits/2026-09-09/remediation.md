# Security audit remediation

This document tracks changes to the findings in [the original audit](report.md), which describes revision `affd95cf048ce48e7919f7da9db7393a848ca317`. The fixes are in the working tree; the original audit is retained as historical evidence. Concurrent IBKR, registry, and CLI work has been preserved.

| Finding | Implemented remediation |
| --- | --- |
| F01 | Signed short-sale/covers, checked Decimal arithmetic, correct exposure and retained short collateral, side-aware stop slippage. |
| F02 | Atomic transfers validate spendable funds and update both account projections; positive withdrawal convention; protected CLI authorization. Distribution withdrawals use the same convention. |
| F03 | Financial workflows use savepoints that reserve the SQLite writer before reading preconditions; failure rolls back related writes and clears facade caches. |
| F04 | Manual financial mutations reload persisted state and validate transitions before writing; stale fill/close replay cannot duplicate cash movements. |
| F05 | Cumulative entry/exit reconciliation books only cash differences, retains partial-cancel exposure, and releases unused long-entry reserves after cancellation/expiry/rejection. Close processing uses effective position state. Derived execution prices use cash differences when cumulative averages change. |
| F06 | Unsafe Alpaca cancel-then-market close is disabled with a typed retry-safe error before broker mutation. Existing bracket protection is left intact. |
| F07 | Alpaca sync fetches current order references individually and recognizes standalone/replacement exits, including fills on a trade still locally marked Filled. Exact top-level lookups take precedence over stale nested bracket legs. |
| F08 | Version 2 backup includes every application table, including execution/policy/distribution data and the new fee ledger. Required table fields prevent silent omissions. A populated all-table round trip and schema coverage assertion protect the manifest. |
| F09 | Advisor errors strip request URLs; configuration Debug redacts API keys. |
| F10 | Risk-rule read failures propagate; short funding applies level-based caps. |
| F11 | Account/broker-scoped activity IDs persist fee identity and allocation; repeated batches cannot charge again. Unmatched fees remain unallocated instead of using symbol/date guesses. |
| F12 | Submission/close/stop intents and audit appends serialize before broker calls; concurrent submission and stale reconciliation are guarded. Delayed submission, close, stop, target, and cancellation acknowledgements cannot overwrite a newer persisted snapshot. Close processing markers are checked inside the financial transaction. |
| F13 | Unknown stop/target modification outcomes retain durable pending intents and block another same-leg change until sync observes the requested price. A pending target change also blocks market close. Adapter and SQLite updates preserve live stop prices. Stop and target mutations use persisted account identity rather than caller-supplied broker/environment metadata. |
| F14 | Audit and synthetic execution provenance use the persisted account's broker identity. |
| F15 | Restore dry-run executes real restore constraints then rolls back; financial/domain validation rejects invalid enums, fee amounts/identities, policy ranges/sums, counts, and cumulative quantity bounds. File export uses a private temporary file and atomic rename. |
| F16 | Installer requires a valid checksum before extraction; release actions are pinned, toolchain version explicit, and release jobs depend on reusable CI. |
| F17 | Preserved the concurrent facade registry fix and its regression coverage; multiple adapters no longer nest behind one fallback identity. |

## Operational changes

- Startup transactionally applies the embedded `2026-09-09-000001_fee_activities` migration when it is the sole pending migration. It only creates the fee ledger and preserves existing rows. Older schemas fail preflight and require the repository migration workflow: first make a consistent SQLite backup and validate the upgrade on a copy. SQLite `file:` URLs use the same schema-based preflight. No user database was migrated during this task.
- Complete JSON backups now use version 2. Earlier incomplete JSON backups are rejected rather than treated as complete recovery copies; keep their original databases for recovery.
- Historical short-trade and negative-withdrawal rows are not automatically rewritten. Reconcile affected historical balances, distributions, and fees against broker records before relying on them.
- Alpaca bracket market-close remains unsupported. An explicitly priced target replacement or broker-side action followed by sync is required; the code no longer cancels bracket protection to attempt a separate market exit.
- Fee and execution ingestion still depend on broker responses. Unattributed fee records and historical lost broker references need explicit reconciliation; passing local tests cannot establish that a live account is fully reconciled.

## Verification

Verification uses temporary source copies, an isolated HOME and database, disabled keychain access, and macOS sandbox rules denying external network and known local broker gateway ports. Loopback ephemeral HTTP mocks are allowed. No ignored live-broker tests, live account operations, deployments, or production data changes are performed.

- `rustup run 1.93.0 cargo test --offline --locked --all-features --workspace --no-fail-fast -- --test-threads=1`: **2,427 passed, 0 failed, 6 ignored**.
- `rustup run 1.93.0 cargo test --offline --locked --no-default-features --workspace --no-fail-fast -- --test-threads=1`: **2,427 passed, 0 failed, 6 ignored**.
- After all final review follow-ups, including target claims and startup upgrades: all-features workspace regression run **2,404 passed, 0 failed**, including **17 security regressions** and **194 SQLite tests**. The 31 unchanged large portfolio/performance cases were excluded from this focused rerun after passing in both full runs above.
- Final Alpaca mapping follow-up: **118 passed**, including conflicting nested/direct fill and price snapshots.
- Final backup tests: **20 passed**, including preservation of an existing backup on export failure and populated all-table round-trip coverage.
- Rust 1.93.0 workspace Clippy with all targets/features and `-D warnings`: passed. Rust 1.93.0 formatting check and `git diff --check`: passed.
- Installer integrity tests: **3 passed**. `actionlint` validation of all release/CI workflows: passed. CLI JSON snapshot tests passed without refreshing snapshots.

The shell initially resolved Homebrew Cargo/Rust 1.97.1 despite `rust-toolchain.toml`; those exploratory results were followed by the explicit Rust 1.93.0 runs above. Test processes remained isolated throughout. The compiler reports an existing future-incompatibility warning for `proc-macro-error2 2.0.1`; it does not fail the pinned-toolchain gates.

The full coverage thresholds, dedicated timing/performance gate, release builds, and cross-platform execution were not rerun. No claim of live-trading certification or repair of preexisting historical ledger errors is made.

## Independent review

The `autoreview` skill ran its isolated structured reviewer with `--mode local --max-priority P2 --no-web-search --prompt-file docs/audits/2026-09-09/remediation-scope.md`. Accepted findings were checked against the code, fixed, and regression-tested: explicit toolchain pinning, IBKR market/stop price parsing, stop-slippage classification, backup semantic validation, persisted mutation identity, stale acknowledgement guards, unused long-entry reserves, durable target claims, and an enforced additive fee-table upgrade. The unused internal reconciliation wrapper was removed so the validated transactional sync path is the sole caller.

One proposed short-exposure fix was rejected: both actual trade-exposure fallback reducers reset exposure on a close; they do not subtract the signed cover amount. `signed_short_cover_clears_fallback_exposure` verifies a -1,000 short entry followed by a -800 cover produces zero remaining exposure.

A later cancellation-account finding was also rejected: the function accepts no caller-supplied Account and loads the account through the persisted trade. `cancellation_ignores_a_stale_caller_trade_account_id` verifies cancellation and cash return still use the correct account when the supplied Trade carries a forged account UUID.

Final structured follow-up review: **clean; no accepted/actionable findings**. The preceding full review had one remaining accepted finding (Alpaca nested/direct snapshot precedence); the final follow-up reviewed that correction and the deterministic target-concurrency test change against the already-reviewed snapshot. TruffleHog also passed for the reviewed changes. See [the final review result](final-review.md).

Final follow-up command (executed in the isolated review snapshot):

```sh
/Users/mati/.agents/skills/autoreview/scripts/autoreview \
  --mode local --max-priority P2 --no-web-search \
  --prompt-file docs/audits/2026-09-09/final-review-scope.md
```
