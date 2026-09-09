# CLI / IBKR feature acceptance scenarios

This is a scenario inventory, **not a claim that every scenario passed**.
The CLI unit test compares these rows to the actual Clap leaf-command tree.
Help/parser coverage does not establish functional or broker coverage.

## Execution lanes

- L: local isolated database, no broker request. Run workspace unit/integration tests and IBKR-profile binary workflows.
- I: interactive workflow. Test domain/dispatcher contracts automatically, then exercise the dialog in a disposable terminal fixture. Never use the real keychain for tests.
- P: authenticated paper gateway read/metadata request. Record entitlement, unsupported capability, and external-provider limitations separately from success.
- M: mocked broker lifecycle first, then explicitly authorized paper order lifecycle. Confirm paper identity immediately before mutation. Never use a live-money account.

Every scenario needs positive, invalid-input, account-isolation, and applicable authorization tests.
For mutations assert persisted state, exact balances, actor/broker provenance, and failure rollback—not only exit status.
For broker timeouts retain unknown outcome, query stable references, and do not blindly retry.
A paper fill is market-dependent; an unfilled or ambiguous submission is not a passed lifecycle test.

## Complete command inventory

| Command | Lane | Acceptance scenario |
| --- | --- | --- |
| `db export` | L | Export a populated ledger; verify schema, exact decimals, audit rows and no credentials. |
| `db import` | L | Restore into a fresh DB; compare ledger and audit; reject malformed backups and protected replacement. |
| `keys create` | I | Create credentials in an isolated fake store; never echo secret values. |
| `keys show` | I | Show environment and redacted metadata only. |
| `keys delete` | I | Delete only fixture credentials; require protected authorization. |
| `keys protected-set` | L | Set fixture keyword; reject unauthorized mutation. |
| `keys protected-show` | L | Display configured status without revealing keyword. |
| `keys protected-delete` | L | Delete fixture keyword only with authorization. |
| `accounts create` | L | Create IBKR paper profile; retain exact broker identity and reject duplicates. |
| `accounts search` | I | Select fixture account by name; verify identity. |
| `accounts list` | L | List IBKR and Alpaca fixtures without mixing identities. |
| `accounts balance` | L | Verify available/reserved/in-market ledger after mutations. |
| `accounts transfer` | L | Transfer between fixture ledgers; verify conservation and rollback. |
| `accounts delete` | L | Reject funded account deletion; delete empty fixture with authorization. |
| `transaction deposit` | L | Record exact Decimal deposit; reject zero, negative and unauthorized input. |
| `transaction withdraw` | L | Record withdrawal; reject overdraft and preserve reserved capital. |
| `rule create` | I | Create risk rule on fixture; validate value and protected gate. |
| `rule remove` | I | Remove fixture rule; verify persistence and protected gate. |
| `trading-vehicle create` | P | Import stock, ETF and bond metadata; validate conid and unsupported categories. |
| `trading-vehicle update-bond-terms` | L | Update coupon/maturity/frequency; reject invalid terms without partial writes. |
| `trading-vehicle search` | L | Filter seeded securities by symbol/category and verify exact results. |
| `trading-vehicle stats` | L | Verify counts by category on fixture. |
| `trade create` | L | Create long/short/bond drafts; check price ordering, Decimal quantity and risk. |
| `trade search` | L | Filter lifecycle states; return only matching account trades. |
| `trade fund` | L | Reserve capital atomically; reject duplicate funding and over-risk. |
| `trade cancel` | M | Cancel bracket; confirm broker terminal state before releasing capital; retry safely. |
| `trade submit` | M | Submit numeric bracket with stable references; verify all three IDs, paper identity and audit. |
| `trade sync` | M | Reconcile partial/full fills, fees and terminal states idempotently; reject wrong identities. |
| `trade watch` | I | Bound polling and interrupt cleanly; verify same reconciliation invariants as sync. |
| `trade manually-fill` | I | Record fixture fill; check transition, balances and duplicate rejection. |
| `trade manually-stop` | I | Close fixture at stop; verify loss accounting and audit. |
| `trade manually-target` | I | Close fixture at target; verify profit distribution and audit. |
| `trade manually-close` | M | Verify safe capability result and no orphaned live protective orders. |
| `trade modify-stop` | I | Amend fixture/paper stop; numeric payload, risk validation and broker confirmation. |
| `trade modify-target` | I | Amend fixture/paper target; numeric payload and broker confirmation. |
| `trade list-open` | L | List only open states for selected account. |
| `trade reconcile` | M | Recover known references after ambiguous submission; never blindly resubmit. |
| `trade size-preview` | L | Verify exact risk sizing with no financial mutation. |
| `trade hypothesis` | L | Verify long/short projections and invalid-price errors without mutation. |
| `trade advisor` | P | Exercise broker-backed analysis; identify missing external calendar credentials explicitly. |
| `trade events add` | L | Append fixture event; preserve actor/time/payload and reject invalid trade. |
| `trade events list` | L | Read events in stable order and isolate trades. |
| `trade audit` | L | Verify hash chain, broker/actor provenance and tamper detection; export without overwriting. |
| `trade autopsy` | L | Generate closed-trade analysis; reject invalid state without changing ledger. |
| `distribution configure` | L | Validate allocation totals and fixture hierarchy; protect mutation. |
| `distribution execute` | L | Distribute positive realized profit exactly once and atomically; reject invalid amounts. |
| `distribution history` | L | Read distribution history and verify conserved totals. |
| `distribution insurance` | L | Verify insurance balance from fixture ledger. |
| `distribution rules show` | L | Read effective allocation rules for selected account. |
| `report performance` | L | Verify closed-trade statistics and empty account. |
| `report drawdown` | L | Verify realized drawdown with known loss sequence. |
| `report risk` | L | Verify open-position exposure and reserved capital. |
| `report concentration` | L | Verify sector and asset-class grouping. |
| `report summary` | L | Verify totals against ledger and JSON snapshot. |
| `report metrics` | L | Verify expectancy, profit factor and zero denominators. |
| `report attribution` | L | Group known P&L by symbol, sector and class. |
| `report benchmark` | P | Compare deterministic dates against broker bars; report missing data explicitly. |
| `report timeline` | L | Verify cashflow bucket boundaries and exact totals. |
| `report bias-summary` | L | Verify behavioral-event counts from fixture. |
| `report wash-sales` | L | Verify cross-trade loss windows using deterministic dates. |
| `report tax` | L | Verify year/currency filters and realized figures. |
| `market-data snapshot` | P | Fetch paper stock/ETF snapshot; check IBKR provenance and missing fields. |
| `market-data bars` | P | Fetch all supported timeframes; validate dates, OHLC and volume. |
| `market-data stream` | P | Exercise bounded stream/polling or explicit unsupported capability; no silent substitution. |
| `market-data quote` | P | Verify bid/ask prices, sizes and broker provenance. |
| `market-data trade` | P | Verify latest trade timestamp, price and missing-data handling. |
| `market-data session` | P | Verify exchange session/timezone behavior. |
| `grade show` | L | Grade closed fixture trade; verify persistence and regrade behavior. |
| `grade summary` | L | Aggregate grades with account/date filters. |
| `level status` | L | Verify fixture current level and risk multiplier. |
| `level triggers` | L | List stable supported trigger identifiers. |
| `level history` | L | Verify chronological change history. |
| `level change` | L | Protected level change persists event atomically. |
| `level evaluate` | L | Test upgrade/downgrade/unchanged branches and apply gate. |
| `level progress` | L | Verify threshold progress and invalid metric inputs. |
| `level rules show` | L | Show effective defaults/overrides. |
| `level rules set` | L | Validate and persist override; reject unauthorized/invalid values. |
| `metrics advanced` | L | Verify advanced metrics and CSV/JSON/text output. |
| `metrics compare` | L | Compare fixture time periods with account filtering and no cross-account data leakage. |
| `metrics bond` | L | Verify Decimal accrued interest/yield with invalid terms rejected. |
| `advisor configure` | L | Validate limits and redact provider secrets. |
| `advisor check` | L | Check concentration limits using fixture holdings. |
| `advisor status` | L | Read effective configuration without secrets. |
| `advisor history` | L | Read persisted check history with date filters. |
| `session open` | I | Capture fixture plan; reject duplicate active session. |
| `session close` | I | Capture review; close only selected active session. |
| `session list` | L | Verify account-scoped session history. |
| `onboarding init` | I | Initialize isolated fixture and verify idempotence/protected setup. |
| `onboarding status` | L | Report readiness accurately without exposing credentials. |
| `policy` | L | Verify machine-readable safety policy and version. |

## Repeatable local verification

Run from the repository root; database-sensitive tests are serialized:

```sh
cargo test --locked --workspace --no-fail-fast -- --test-threads=1
cargo test --locked --no-default-features --workspace --no-fail-fast -- --test-threads=1
cargo test --locked -p trust-cli --test integration_test_dispatch_matrix -- --test-threads=1
cargo fmt --all -- --check
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
make ci-snapshots
```

The dispatch matrix runs local workflows for both Alpaca and IBKR profiles; it is not a gateway test.
Root help/version, nested help, global zen, invalid command/flag handling, and JSON error contracts are additional cross-cutting checks.
Use disposable database paths and disable keychain access; never restore a test backup over the paper ledger.

## Paper execution and evidence

Use the existing live-test preflight and opt-in paper-order targets in the Makefile only after checking their required environment variables and account gates.
Run each applicable P scenario with the built trust binary; record command (redacted), exit code, structured outcome and source revision.
Before M scenarios, verify authentication, selected account, paper status, absence of competing session, and known outstanding orders.
Exercise submit → reconcile → amend → cancel, and separately partial/full fills → fees → close where supported.
Do not mark an unsupported capability, missing entitlement, or ambiguous response as successful.
Verify audit-chain integrity before and after each lifecycle, and retain historical records even if a prior version recorded incorrect provenance.
Application audit records are not a complete network/security audit: read commands, access failures, log retention, external anchoring and tamper resistance need their own evidence.
