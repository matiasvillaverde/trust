**Trust technical and security audit — 9 September 2026**

**Verdict: this revision does not meet a high-assurance live-trading safety bar.** The strongest findings concern incorrect accounting, incomplete broker recovery, missing backup data, and credential exposure. Passing the existing suite does not establish these invariants: isolated reproductions demonstrate failures the suite currently misses.

Reviewed revision: `affd95cf048ce48e7919f7da9db7393a848ca317`, workspace version 0.3.5, Rust 1.93.0. The worktree was clean at the start. This audit added only its report and evidence artifacts. Concurrent edits later appeared in `core/src/lib.rs`, `core/src/broker_registry.rs`, and `ibkr-broker/src/client.rs`; they were left untouched. Findings and test counts describe the fixed revision above, not a certification of the evolving worktree. The audit did not change application source, configuration, migrations, credentials, or trading data.

The audit identifies **14 high-priority findings and 3 medium-priority findings**. Priority describes potential harm to financial integrity, safe broker operation, or confidentiality; it is not a CVSS score or a claim of unauthenticated remote exploitation. Twelve added tests reproduce specific defects in a temporary copy. Other findings are supported by traced source paths and, where relevant, upstream documentation.

**Scope, trust boundaries, and method**

All eight crates were included in inventory, compilation/linting, and targeted review. The repository contains 230 Rust files across these crates, totaling 137,130 lines including tests. This was a broad source audit with deeper tracing of high-risk workflows, not a claim that every line received equal manual scrutiny.

The relevant boundaries are CLI input → facade → model contracts → SQLite/broker adapters; broker and calendar responses → normalization → accounting; local backup files → restore; and release artifacts → installed executable. The assets at risk are broker authority, account/trade balances, order protection, execution and fee history, policy configuration, and API keys.

An authorized CLI user or automation can trigger many of the financial defects without malicious input. Network failures, delayed fills, concurrent CLI processes, stale caller objects, and failed SQLite writes are ordinary operational conditions. A party able to edit the local database or replace the executable already has extensive authority; protected keywords and local hash chains should not be treated as security boundaries against that party.

| Area | Review emphasis | Assessment |
|---|---|---|
| model | Broker/database contracts, amounts, identity, lifecycle states | Decimal-based contracts are useful, but supported states exceed what core reconciles. |
| core | Funding, transfers, fill/close, risk rules, distributions, audit, calculations | Multiple reproduced violations of accounting and risk invariants. |
| db-sqlite | Worker writes, savepoints, foreign keys, migrations, backups | Good local transaction primitives; incomplete workflow atomicity and backup coverage. |
| cli | Dispatch, protected operations, DB paths, output contracts, tests | Broad passing coverage; transfer command reaches unsafe core path. |
| alpaca-broker | Submit/cancel/close, mapping, credentials, activities | Close loses protection and replacement-market fills are missed by sync. |
| ibkr-broker | Session/account checks, order references, TLS, parsing, mutation recovery | Useful preflight checks; core state gaps and stop-recovery gaps remain. |
| broker-sync | Actor, messages, state machine, resource bounds | In-memory scaffold; not a functioning reconciliation service. |
| advisor | Secrets, HTTP errors, correlation/regime and concentration | HTTP errors expose credential-bearing URLs; some core advisory errors become zero exposure. |
| delivery | Cargo dependencies, secret scan, CI/release workflows, installer | No embedded secrets detected; installer integrity verification fails open. |

Builds, tests, and secret scanning used temporary source copies and a macOS sandbox denying all network access. Test processes received an isolated HOME, temporary database paths where applicable, and `TRUST_DISABLE_KEYCHAIN=1`; the real Keychains directory was also denied. No live broker tests, account operations, deployment, credential verification, or real-data experiments were performed. Separate read-only upstream documentation and advisory-database retrieval provided current dependency/API context.

**Findings, ordered for remediation**

**F01 — High: short-trade realized profit and account cash are inverted. Reproduced.**

The fill and close transaction paths use long-style cash-flow signs for both trade directions. The projection reducer subtracts entry cost and adds exit proceeds without consulting `TradeCategory`. A short sale of 10 units at 100, covered at 80, should yield +200 before fees. The reproduction records **−200**, and an initially 10,000 account ends at **9,800** instead of 10,200. Losing shorts can conversely appear profitable, affecting distribution and leveling decisions.

Evidence: [core/src/commands/transaction.rs:184](/Users/mati/Code/matiasvillaverde/trust/core/src/commands/transaction.rs:184), `:356`, `:400`; [core/src/commands/balance.rs:189](/Users/mati/Code/matiasvillaverde/trust/core/src/commands/balance.rs:189); [core/src/lib.rs:1689](/Users/mati/Code/matiasvillaverde/trust/core/src/lib.rs:1689). Proof: `audit_short_profit_is_inverted`.

Repair the authoritative cash-flow model for long and short trades together, including reserved collateral, price improvement, fees, account projections, and realized P&L. Require long/short lifecycle conservation tests before changing projections or migrating historical results.

**F02 — High: inter-account transfers overspend, use inconsistent signs, and leave balances stale. Reproduced.**

`FundTransferService` validates positive amount and hierarchy, but does not check spendable funds. SQLite atomically inserts two rows yet does not update either account projection. The withdrawal row is negative, while the available-capital calculator subtracts withdrawal amounts, so recomputation can increase the source's available funds. The reproduction successfully transfers **15,000 from a 10,000 account** while its cached available balance stays **10,000**. The CLI exposes this path through `accounts transfer`; that handler also lacks the protected-keyword check used for deposits/withdrawals.

Evidence: [core/src/services/fund_transfer_service.rs:26](/Users/mati/Code/matiasvillaverde/trust/core/src/services/fund_transfer_service.rs:26); [db-sqlite/src/database.rs:453](/Users/mati/Code/matiasvillaverde/trust/db-sqlite/src/database.rs:453); [core/src/calculators_account/capital_available.rs:20](/Users/mati/Code/matiasvillaverde/trust/core/src/calculators_account/capital_available.rs:20); [core/src/lib.rs:2292](/Users/mati/Code/matiasvillaverde/trust/core/src/lib.rs:2292); [cli/src/dispatcher.rs:10122](/Users/mati/Code/matiasvillaverde/trust/cli/src/dispatcher.rs:10122). Proof: `audit_transfer_overspends_and_leaves_projection_stale`.

Use one transaction to validate persisted accounts and currency, reserve/check funds, insert consistently signed ledger entries, and update both projections. Apply the intended protected-operation policy at the facade boundary. Test that ledger reconstruction and cached balances agree after successful and rejected transfers.

**F03 — High: local financial workflows are not atomic. Reproduced.**

Funding sets status to `Funded` before writing its funding transaction and balance changes. Injecting a failure on insertion of `fund_trade` leaves a persisted **Funded trade with funding 0**. Submission's validator checks status, so this inconsistent state can reach submission. Deposits, withdrawals, funded cancellation, manual fill/close, and trade creation similarly span multiple independently committed worker calls. Broker-sync savepoints do not protect these other entry points.

Evidence: [core/src/commands/trade.rs:658](/Users/mati/Code/matiasvillaverde/trust/core/src/commands/trade.rs:658), `:385`, `:289`, `:94`; [core/src/commands/transaction.rs:60](/Users/mati/Code/matiasvillaverde/trust/core/src/commands/transaction.rs:60), `:112`; [core/src/validators/trade.rs:8](/Users/mati/Code/matiasvillaverde/trust/core/src/validators/trade.rs:8). Proof: `audit_funding_failure_leaves_funded_status`.

Enclose each entirely local business mutation—including its validation—in a transaction/savepoint with complete rollback handling. Include post-close financial processing. Exercise failures after every financial write, not only distribution failures.

**F04 — High: manual fill and close accept stale state and replay accounting. Reproduced.**

`TrustFacade::fill_trade` passes the caller's `Trade` directly into financial mutations. Unlike fund/submit/cancel, the manual path does not reload it. A previously Funded snapshot continues to pass fill validation after the persisted trade is Filled. Repeating that call succeeds and records **2,000 in-market capital for a 1,000 fill**, reducing account total balance a second time. Manual stop/target methods also lack persisted lifecycle validation; close validation checks the amount rather than the prior trade status.

Evidence: [core/src/lib.rs:1568](/Users/mati/Code/matiasvillaverde/trust/core/src/lib.rs:1568), `:1586`, `:1669`; [core/src/commands/trade.rs:289](/Users/mati/Code/matiasvillaverde/trust/core/src/commands/trade.rs:289), `:363`, `:374`; [core/src/validators/transaction.rs:8](/Users/mati/Code/matiasvillaverde/trust/core/src/validators/transaction.rs:8), `:73`. Proof: `audit_stale_fill_replays`.

Reload authoritative state inside the transaction, enforce an explicit transition table, and give each financial event a durable idempotency key. Test repeated calls with both old and refreshed snapshots.

**F05 — High: partial fills and terminal unfilled outcomes cannot be reconciled. Reproduced in the transition validator.**

The model and IBKR mapper emit `PartiallyFilled`, `Canceled`, `Expired`, and `Rejected`, but core's sync transition validator accepts none of these incoming states. `update_status` lacks their accounting paths too. A legitimate partial-fill sync rolls back rather than persisting exposure/executions. The Alpaca path queries only closed orders and maps changed nonfilled entry orders to Submitted, further limiting visibility. A subsequent full fill may reconcile, but the intervening exposure and terminal cancellation cases remain unhandled.

Evidence: [core/src/commands/trade.rs:1246](/Users/mati/Code/matiasvillaverde/trust/core/src/commands/trade.rs:1246), `:196`, `:814`; [ibkr-broker/src/orders.rs:216](/Users/mati/Code/matiasvillaverde/trust/ibkr-broker/src/orders.rs:216); [alpaca-broker/src/sync_trade.rs:62](/Users/mati/Code/matiasvillaverde/trust/alpaca-broker/src/sync_trade.rs:62); [alpaca-broker/src/order_mapper.rs:112](/Users/mati/Code/matiasvillaverde/trust/alpaca-broker/src/order_mapper.rs:112). Proof: `audit_partial_fill_transition_rejected`.

Specify execution-driven accounting for partial entry/exit fills, partial-fill cancellation, rejection, and expiry. Preserve reserved funds until zero exposure is established; support reconciliation throughout the lifecycle rather than expanding the enum match alone.

**F06 — High: Alpaca close cancels protective orders before securing an exit. Source and upstream contract.**

Alpaca close deletes the target and then submits a separate market order. Alpaca documents that canceling any bracket member cancels remaining open members. Thus the protective stop can disappear before the market exit succeeds. A request rejection or failure between steps leaves an unprotected position. Core nevertheless records `protective_stop_retained: true`. The adapter also sizes the new exit from original entry quantity, without reconciling remaining exposure after partial exits or a cancel/fill race.

Evidence: [alpaca-broker/src/close_trade.rs:14](/Users/mati/Code/matiasvillaverde/trust/alpaca-broker/src/close_trade.rs:14), `:90`; [core/src/commands/trade.rs:1764](/Users/mati/Code/matiasvillaverde/trust/core/src/commands/trade.rs:1764). Contract: [Alpaca order documentation](https://docs.alpaca.markets/us/docs/orders-at-alpaca).

Design a broker-specific close workflow with confirmed remaining quantity, durable exit references, explicit cancellation outcomes, and a recoverable failure path. Do not claim stop protection in the audit unless the broker confirms it. Validate with a deterministic broker simulator before any authorized paper exercise.

**F07 — High: Alpaca reconciliation ignores the standalone market-close fill. Reproduced.**

Core now leaves a close-requested trade Filled and changes its target to a new market-order ID. Alpaca sync searches a top-level target only for Canceled trades. For Filled trades it finds the original entry and examines that bracket's children. The new standalone market exit is outside that bracket. A fixture containing the original bracket and a fully filled replacement market order still returns **Filled**, with no target fill update. Cash release and close processing can remain stuck indefinitely.

Evidence: [alpaca-broker/src/sync_trade.rs:30](/Users/mati/Code/matiasvillaverde/trust/alpaca-broker/src/sync_trade.rs:30); [alpaca-broker/src/order_mapper.rs:11](/Users/mati/Code/matiasvillaverde/trust/alpaca-broker/src/order_mapper.rs:11); [core/src/commands/trade.rs:1796](/Users/mati/Code/matiasvillaverde/trust/core/src/commands/trade.rs:1796), `:1848`. Proof: `audit_standalone_market_close_is_ignored`.

Reconcile each current order reference, including separately submitted exits and replacement history. Cover accepted, lost-acknowledgment, filled, and partially filled standalone exits.

**F08 — High: “full” JSON backups omit five live tables. Reproduced.**

The serialized envelope, export, and restore omit **executions, distribution_rules, distribution_history, level_adjustment_rules, and advisory_thresholds**. Restoring into a fresh database silently loses execution evidence and safety/distribution policy. Replace mode also omits these tables from child-first deletion; populated foreign-key references can make replacement fail. Existing round-trip tests cover the envelope's tables, not all schema tables.

Evidence: [db-sqlite/src/backup.rs:92](/Users/mati/Code/matiasvillaverde/trust/db-sqlite/src/backup.rs:92), `:469`, `:535`, `:748`. Proof: `audit_backup_omits_execution_and_policy_tables`.

Version the complete backup contract and audit every table through export, clear, import, and compatibility behavior. Add a schema-to-backup coverage assertion and a round-trip test containing rows in every live table. Existing JSON exports should not be represented as complete recovery copies.

**F09 — High: advisor failures expose API keys. Reproduced with a synthetic key.**

Calendar requests put `apikey`/`apiKey` in the URL. `send()?` converts reqwest failures via `AdvisorError::Http(value.to_string())`, preserving the request URL and query credential. The error is then suitable for CLI display/logging. The sandboxed reproduction confirms the synthetic key appears in the public error string. Separately, `AdvisorConfigUpdate` derives Debug while containing raw secret fields, creating another disclosure path if logged.

Evidence: [advisor/src/catalyst.rs:175](/Users/mati/Code/matiasvillaverde/trust/advisor/src/catalyst.rs:175), `:201`; [advisor/src/error.rs:74](/Users/mati/Code/matiasvillaverde/trust/advisor/src/error.rs:74); [advisor/src/config.rs:160](/Users/mati/Code/matiasvillaverde/trust/advisor/src/config.rs:160). Proof: `audit_http_error_exposes_query_secret`.

Remove URLs from HTTP errors before formatting, use redacted Debug implementations for every secret-bearing type, and test failures—not only configuration status output. Keep query credentials and raw provider messages out of logs.

**F10 — High: unreadable risk rules become approval for shorts; short sizing skips level caps. Reproduced for rule failure.**

`sorted_rules` converts every rule-read error into an empty list. Long funding subsequently rereads rules through the sizing calculator, but short funding returns early from level-adjusted sizing. The reproduction first rejects a short under a tight risk rule, then accepts the same trade after the rules table becomes unreadable. Independently, that early return means normal short funding never enforces the level multiplier.

Evidence: [core/src/validators/funding.rs:69](/Users/mati/Code/matiasvillaverde/trust/core/src/validators/funding.rs:69), `:120`; [core/src/calculators_trade/quantity.rs:88](/Users/mati/Code/matiasvillaverde/trust/core/src/calculators_trade/quantity.rs:88). Proof: `audit_short_funding_ignores_rule_read_failure`.

Distinguish “no configured rule” from read/parse failure and fail closed on the latter. Implement side-aware sizing so short trades receive the same level-policy enforcement as longs.

**F11 — High: fee attribution and incremental reconciliation are not event-idempotent. Reproduced for wrong-order attribution.**

A fee with a broker order ID belonging to another trade can fall through to symbol/date matching and be charged to this trade. Two same-symbol trades can each receive the same fee. There is no persisted activity-ID deduplication in the reconciliation path. Additionally, sync fetches fees after the latest execution timestamp, then compares that incremental batch's amount with all historically booked fees. Previously booked 1 followed by a new batch of 2 books only 1 more, leaving total 2 instead of 3. A shared execution cursor can also exclude delayed fees.

Evidence: [core/src/commands/trade.rs:823](/Users/mati/Code/matiasvillaverde/trust/core/src/commands/trade.rs:823), `:1094`, `:1143`; [alpaca-broker/src/fees.rs:90](/Users/mati/Code/matiasvillaverde/trust/alpaca-broker/src/fees.rs:90); [ibkr-broker/src/executions.rs:118](/Users/mati/Code/matiasvillaverde/trust/ibkr-broker/src/executions.rs:118). Proof: `audit_fee_for_unrelated_order_is_charged`.

Persist broker activity identity and attribution once per account, use a fee-specific cursor/replay window, and treat a mismatching explicit order ID as exclusion. Ambiguous account-level fees need an explicit allocation policy rather than independent per-trade guesses.

**F12 — High: durable mutation intent does not exclude concurrent callers. Source-based.**

Submit and close reload state, validate, and later write intent through unconditional updates. There is no compare-and-set on prior status/order version or process-wide/account-wide transaction encompassing the decision. Two CLI processes can both observe Funded or an unmodified target before either claims it, and both cross the broker boundary. Broker-specific deduplication may mitigate some requests, but is not a general exclusion guarantee; the Alpaca market-close request has no stable client order ID. Concurrent audit appends can also share a previous hash and produce a fork.

Evidence: [core/src/commands/trade.rs:685](/Users/mati/Code/matiasvillaverde/trust/core/src/commands/trade.rs:685), `:1764`; [db-sqlite/src/workers/worker_trade.rs:332](/Users/mati/Code/matiasvillaverde/trust/db-sqlite/src/workers/worker_trade.rs:332); [core/src/audit.rs:80](/Users/mati/Code/matiasvillaverde/trust/core/src/audit.rs:80); [alpaca-broker/src/close_trade.rs:90](/Users/mati/Code/matiasvillaverde/trust/alpaca-broker/src/close_trade.rs:90).

Claim operations transactionally using expected status/version and verify exactly one affected row. Persist stable broker idempotency keys and unresolved operation state. Add two-process tests for duplicate funding, submission, exit, audit append, and account risk-limit races. Do not hold an ordinary SQLite write transaction across slow network calls.

**F13 — High: an uncertain stop modification can undermine the no-widening rule. Source-based.**

Stop modification validates against the persisted price, calls the broker, then writes the new price/ID. If remote tightening succeeds but its acknowledgment or local persistence fails, there is no durable pending-price restriction. A long stop tightened from 90 to 95 may still appear locally as 90; a later request for 92 passes local validation while widening actual broker risk. IBKR resolves the same stable order reference and applies that request. Sync does not repair this: IBKR's mapper and core's merge omit current stop/limit price fields.

Evidence: [core/src/commands/trade.rs:486](/Users/mati/Code/matiasvillaverde/trust/core/src/commands/trade.rs:486), `:1280`; [core/src/validators/trade.rs:85](/Users/mati/Code/matiasvillaverde/trust/core/src/validators/trade.rs:85); [ibkr-broker/src/lib.rs:256](/Users/mati/Code/matiasvillaverde/trust/ibkr-broker/src/lib.rs:256); [ibkr-broker/src/orders.rs:178](/Users/mati/Code/matiasvillaverde/trust/ibkr-broker/src/orders.rs:178).

Persist pending modification intent and the strongest authorized stop before I/O; block conflicting mutations until reconciled. Reconcile live order parameters as well as status and executions, and enforce no-widening against confirmed/pending broker state.

**F14 — Medium: the broker registry mislabels IBKR audit and synthetic execution records. Source-based.**

The CLI registers Alpaca first. `BrokerRegistry::kind()` always reports that fallback kind, although actual calls route by account. Core uses `broker.kind()` to label audit entries and derive execution records. Attempted IBKR operations therefore acquire Alpaca audit labels. The same identity is used by synthetic execution derivation, damaging provenance and potentially splitting deduplication namespaces once routing succeeds. F17 separately prevents successful default-CLI IBKR routing in the audited revision; fixing that construction defect alone does not fix this identity bug.

Evidence: [cli/src/dispatcher.rs:539](/Users/mati/Code/matiasvillaverde/trust/cli/src/dispatcher.rs:539); [core/src/broker_registry.rs:70](/Users/mati/Code/matiasvillaverde/trust/core/src/broker_registry.rs:70); [core/src/commands/trade.rs:694](/Users/mati/Code/matiasvillaverde/trust/core/src/commands/trade.rs:694), `:820`, `:911`.

Use the validated account's broker identity, or an operation-scoped resolved broker, for both routing and persisted provenance. Test a registry with both adapters and nondefault accounts.

**F15 — Medium: restore dry-run reports success without exercising restore constraints. Reproduced.**

Dry-run exits after metadata and UUID checks. It does not enforce foreign keys or strict-mode emptiness. A backup containing an account whose parent does not exist passes dry-run and fails actual import. Decimal and domain-value validation is also absent from the row-validation helper, allowing structurally valid but unusable values to reach the database where SQL constraints permit them.

Evidence: [db-sqlite/src/backup.rs:658](/Users/mati/Code/matiasvillaverde/trust/db-sqlite/src/backup.rs:658), `:748`. Proof: `audit_dry_run_accepts_broken_foreign_key`.

Perform validation in a disposable migrated database, or execute the actual restore under a guaranteed rollback with equivalent constraints. Make dry-run match actual mode semantics, and validate financial/domain values before claiming restorability.

**F16 — Medium: installation continues without artifact verification. Source-based.**

A missing checksum download or missing checksum utility only prints a warning; extraction and installation continue. This defeats verification precisely when the verification artifact or tooling is unavailable. Checksums fetched from the same release also do not independently authenticate a compromised release. Release workflows use mutable action tags, and their own job graphs do not require the CI safety gates before publishing.

Evidence: [install.sh:73](/Users/mati/Code/matiasvillaverde/trust/install.sh:73); [.github/workflows/release.yaml:15](/Users/mati/Code/matiasvillaverde/trust/.github/workflows/release.yaml:15); [.github/workflows/release-on-version-bump.yaml:48](/Users/mati/Code/matiasvillaverde/trust/.github/workflows/release-on-version-bump.yaml:48).

Require checksum verification, verify release provenance/signatures where provided, pin privileged actions to reviewed commit SHAs, and tie published artifacts to a revision that passed required gates. This audit did not inspect remote branch protection or GitHub account settings.

**F17 — High: nested broker registries prevent default-CLI IBKR routing. Reproduced on the audited revision.**

`new_with_brokers` constructs a multi-broker registry and passes it to `TrustFacade::new`, which wraps that registry again as a single broker keyed by its fallback kind. With the CLI's Alpaca-first registration, the outer registry knows only Alpaca. An IBKR account therefore receives “not configured” before reaching its adapter, despite both adapters being supplied. Direct registry tests miss the extra facade wrapper. This affects market-data and trading/reconciliation calls and can prevent management of an IBKR position through the CLI.

Evidence: [core/src/lib.rs:345](/Users/mati/Code/matiasvillaverde/trust/core/src/lib.rs:345), [core/src/lib.rs:359](/Users/mati/Code/matiasvillaverde/trust/core/src/lib.rs:359); [core/src/broker_registry.rs:17](/Users/mati/Code/matiasvillaverde/trust/core/src/broker_registry.rs:17); [cli/src/dispatcher.rs:539](/Users/mati/Code/matiasvillaverde/trust/cli/src/dispatcher.rs:539). Proof: `audit_nested_registry_blocks_ibkr`.

Construct exactly one registry and test both account kinds through the facade in both registration orders. The concurrent worktree diff introduces a direct registry constructor and such a test; it appears to address this finding, but was not included in the audited snapshot or its baseline verification. Preserve that work and separately validate it before treating F17 as resolved.

**Additional technical limitations and follow-up risks**

- Broker-sync's `StartSync`, `StopSync`, and `ManualReconcile` are no-ops. The connection state machine is separate from the session actor. Unbounded channels, ignored event-send failures, and lack of join/shutdown acknowledgment should be addressed before real broker I/O is wired in. Existing scaffold tests are not evidence of continuous reconciliation.
- Synthetic trade-update executions and account-activity executions have different IDs and can describe the same fill. Incremental synthetic fills use the latest cumulative average as the delta price. Define whether this table is raw observations or a canonical execution ledger before downstream consumers aggregate it; no complete execution-derived accounting guarantee was established.
- Alpaca leg matching compares numeric display strings and suppresses some leg mapping errors. Numerically equal prices with different scale, missing persisted IDs after accepted submission, and the bounded closed-order lookup require recovery tests. These were reviewed as follow-up risks, not counted as reproduced findings.
- IBKR contract selection chooses the first exact symbol, without disambiguating multiple same-symbol results by requested currency/exchange. Saved numeric broker IDs are trusted without a metadata recheck. Add ambiguity rejection and asset/currency identity tests before relying on broader instrument coverage.
- Core advisory concentration helpers replace some arithmetic failures with zero exposure or partial totals. Advisor correlation/regime use Decimal and explicitly handle several missing-data cases, but this does not make every advisory path fail closed.
- The local audit chain checks internal linkage, not authenticity against a writer of the database. Tail deletion, deletion of the entire chain, or a complete rewrite cannot be detected without a separately trusted head/checkpoint. Actor/request identity comes from environment values. No externally anchored audit service was found.
- Backup export performs multiple table reads without an explicit consistent read transaction and writes directly to the destination with `File::create`. A cross-process writer can produce an inconsistent snapshot; failed export can replace a good previous backup, and a destination colliding with the live DB is not rejected. Use a consistent snapshot plus a private temporary file and atomic rename, with database-path collision checks.
- Existing database files are not automatically migrated by `SqliteDatabase::new`; only new files receive embedded migrations. Migration behavior is documented in AGENTS, so this is an operational dependency rather than a newly inferred auto-migration promise. Library connection errors also terminate the process instead of returning typed errors.
- Tax/wash-sale outputs inherit P&L defects. Tax-law correctness across jurisdictions was not certified. Coverage includes implementation review and existing tests, not professional validation of financial or tax policy.

**Verification results and their limits**

| Check | Result |
|---|---|
| Formatting on pristine snapshot | Passed `cargo fmt --all -- --check`. |
| Strict Clippy, workspace/all targets/all features | Passed with `-D warnings`; Cargo reported future incompatibility in proc-macro-error2. |
| Library tests | 983 passed. 16 IBKR HTTP mock tests failed because sandbox denied sockets. Two advisor socket tests were filtered after the same restriction was confirmed. |
| CLI tests | 1,314 passed; 1 ignored. Includes JSON contract/snapshot suites. |
| Core, SQLite, broker-sync integration targets | 59 passed. |
| Added defect reproductions | 12 new tests passed, meaning the asserted defects were observed. The inline run also selected one existing test. |
| Current-snapshot TruffleHog scan | 0 findings; 713 chunks, approximately 6.16 MB; verification and updates disabled. Git history was not scanned. |
| RustSec lockfile audit without repository suppressions | One vulnerability record plus informational warnings; reachability assessed below. |

A total of **2,356 existing tests passed**. This is not a clean full-suite result: 18 socket-dependent cases could not run under the strict isolation policy, and the IBKR standalone HTTP/security integration suites were not executed. No ignored live tests ran. Full coverage/performance gates, release builds, cross-platform execution, and the workspace's complete no-default-features test matrix were not rerun. The audit reproductions are additional evidence, not fixes.

The RustSec database was refreshed to commit `b50980aad8b8f14f77e25a97b32dd94bf008b0af` (9 September 2026). The lockfile contains 356 dependency records. An unsuppressed scan flags rkyv 0.7.46 ([RUSTSEC-2026-0235](https://rustsec.org/advisories/RUSTSEC-2026-0235.html)); `cargo tree --target all -i rkyv` produces no active reverse dependency. Thus this is a locked optional dependency, not demonstrated exposure in the selected workspace feature graph. The narrow existing suppression is reasonable only while that feature remains disabled.

The scan also reports proc-macro-error2 2.0.1 as unmaintained ([RUSTSEC-2026-0173](https://rustsec.org/advisories/RUSTSEC-2026-0173.html)), and rand 0.8.5's conditional custom-logger soundness issue ([RUSTSEC-2026-0097](https://rustsec.org/advisories/RUSTSEC-2026-0097.html)). The host feature tree showed no active reverse dependency for rand 0.8.5; the specific custom-logger exploit conditions were not established. Do not present either informational warning as a confirmed remote vulnerability. Yanked-version checks were disabled, and native/system libraries were not independently vulnerability-scanned.

**Recommended remediation order**

1. Correct ledger direction/sign conventions and transfer projections; introduce atomic validated writes. Preserve existing data for a separately reviewed accounting repair rather than silently recomputing balances.
2. Add persisted-state validation and event idempotency to every lifecycle entry point. Add concurrent operation claims and fault-injection tests.
3. Complete partial-fill/terminal-state accounting and redesign Alpaca close/recovery. Make stop protection and unresolved modifications explicit.
4. Repair fee identity/cursors, provenance labeling, and complete backup/restore behavior.
5. Close credential-error/Debug disclosures and enforce artifact verification.
6. Convert each reproduction into a regression asserting the safe outcome, then run the full repository gates. Any later paper-broker exercise should be separately authorized and use dedicated paper credentials; these results do not justify a live-trading trial.

The safe acceptance criteria are concrete: a rejected financial action changes nothing; a successful action preserves ledger/projection consistency; a broker mutation has one durable owner and a recoverable unknown outcome; actual exposure is represented through partial fills; a stop cannot widen through retries or stale state; backups restore every financial/policy table; and errors reveal no secret material.

**Evidence and reproduction**

[verification.json](verification.json) records check counts and the temporary workspace. [dependencies-unfiltered.json](dependencies-unfiltered.json) retains unsuppressed advisory output. [inventory.json](inventory.json) records per-crate source counts. [proofs.txt](proofs.txt) and [inline-proofs.txt](inline-proofs.txt), and [registry-proof.txt](registry-proof.txt) retain the successful final reproduction runs. [reproductions.patch](reproductions.patch) contains test-only additions used in the temporary copy; it is not applied to this repository's application files.

To inspect or rerun, create a disposable copy of the reviewed revision, apply the patch there, and use a network-denying sandbox and temporary HOME/database paths. The five core integration reproductions run with `cargo test --offline --locked -p core --test audit_findings -- --test-threads=1 --nocapture`. The remaining reproductions run with `cargo test --offline --locked -p core -p alpaca-broker -p advisor -p db-sqlite --lib audit_ -- --test-threads=1`. They intentionally assert defective current behavior; reverse the expectations when implementing fixes.
