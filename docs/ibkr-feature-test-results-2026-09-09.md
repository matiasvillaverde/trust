# CLI / IBKR verification — 2026-09-09

## Acceptance status: incomplete; not a release sign-off

The scenario inventory covers all 90 Clap leaf commands. This does **not** mean all
90 features have passed functional end-to-end tests. Interactive dialogs, broker
capabilities, external providers, and market-dependent order lifecycles require
separate evidence. See [the complete scenarios](ibkr-feature-scenarios.md).

The shared main worktree contains concurrent financial/security changes. Results
below distinguish that tree from the isolated feature patch based on `affd95c`.
No credentials are included in this report. No new paper orders were submitted in
this verification pass; an earlier ambiguous submission remains unresolved.

## Changes driven by this pass

- Added an inventory test that compares the scenario list with the actual command
  tree, plus help parsing for every leaf. These are discovery checks, not functional proof.
- Expanded the real-binary local dispatch matrix to 38 successful command invocations, repeated for
  Alpaca and IBKR profiles. Assertions include clean JSON and account/broker
  provenance. Added missing/wrong authorization checks for distribution mutations.
- Fixed attribution, benchmark, and timeline reports labeling IBKR accounts as Alpaca.
  The attribution regression failed before the fix and passed afterwards.
- Connected distribution configure/execute to protected authorization and exposed
  `--confirm-protected`. Configuration now succeeds with authorization; missing or
  incorrect keywords fail. Successful distribution execution still needs broader
  accounting/lifecycle verification.
- Fixed fractional historical volume silently becoming zero. Conversion uses
  Decimal and the response multiplier, rejecting overflow, invalid values, and
  nonintegral scaled results. Regression tests include the observed fractional
  response and invalid factors. IBKR documents the multiplier in its
  [historical data response contract](https://www.interactivebrokers.com/docs/web-api/v1/endpoints/market-data/historical-market-data).
- Added an opt-in, read-only gateway test using the built CLI and a disposable
  database. It checks IBKR/account provenance; historical bars also require
  nonempty data, positive prices, and nonzero traded volume in the fixed window.

## Recorded results

| Verification | Result |
| --- | --- |
| Latest main workspace rerun, default features, no fail-fast | 2,364 passed; 54 failed; 6 ignored; 7 failing targets. Includes later concurrent fixes but is still not green. |
| Main workspace, default features, no fail-fast | 2,251 passed; 152 failed; 5 ignored; 14 failing targets. This was an earlier snapshot of the concurrently changing worktree. |
| Main workspace, no default features | 2,272 passed; 143 failed; 6 ignored; 11 failing targets. This snapshot predates later concurrent fixture corrections. |
| Expanded local binary matrix, after fixes | 3 tests passed: 38 successful invocations for each broker plus authorization and market-data/preview negative paths. Some commands repeat to test their lifecycle. |
| Complete leaf inventory and help parsing | 2 tests passed; all 90 leaves represented. |
| IBKR adapter suite, after volume fix | 72 unit, 15 HTTP, 9 security, 3 live-input tests passed; 4 live tests ignored by default. |
| Explicit authenticated gateway preflight | Passed. |
| Opt-in real-binary paper reads, after fixes | Snapshot, quote, latest trade, session response, bars at 1m/1h/1d, and benchmark all passed their current assertions. |
| Historical volume spot check | Previously zero; now 43,912,882 for the observed bar. |
| Paper trade audit verification | Four-event chain verifies; historical incorrect provenance is retained, not rewritten. |
| Realtime IBKR streaming | Explicit unsupported-capability error; not a successful streaming test. |
| Session classification | Returned `unknown`; response/provenance works, exchange-session classification is not established. |
| CLI JSON snapshots | Passed, no snapshot refresh. |
| Formatting and diff whitespace | Passed after final formatting. |
| Main workspace Clippy | Failed in concurrently edited `db-sqlite/src/backup.rs` (`too_many_lines`). |
| Latest main workspace Clippy rerun | Still fails: test length in `core/src/services/fund_transfer_service.rs` and unnecessary Vec in `core/src/commands/trade.rs`. |
| Isolated feature patch Clippy | Passed, IBKR and CLI all targets/all features. |
| Isolated feature patch autoreview | Clean at the default P0 threshold; not a security certification. |
| Isolated feature patch full workspace | 2,408 passed; 3 failed; 6 ignored. Three transfer tests fail because the shared dispatcher snapshot includes a concurrent transfer gate without its companion changes. This is not a release candidate. |

## Open acceptance gaps

- Main has failing cumulative-fill/accounting, lifecycle fixture, backup, and
  other regression tests. A green isolated patch does not make main green.
- The previously submitted paper trade has an unknown outcome. Empty order
  snapshots do not prove rejection. Do not automatically submit again or release
  its reserved capital. Full submit/amend/cancel/fill/fee/close acceptance is pending.
- The read-only bar test checks returned values, not completeness of the requested
  interval. One daily request spanning September 1–3 returned only September 1
  after filtering; endpoint boundary/pagination behavior needs further verification.
- Stock/ETF/bond resolution and complete paper order lifecycles are not all proven
  by this pass. A resolvable bond and suitable paper permissions remain prerequisites.
- Interactive workflows have automated domain/dialog tests, but not a complete
  terminal-driven run for every command. External advisor data requires separate
  provider configuration. Streaming is not implemented by the IBKR adapter.
- Trade hash chains do not audit every CLI read, authentication failure or local
  configuration operation. Retention, restricted access, external anchoring and
  tamper resistance remain separate audit-system acceptance work.

## Reproduction and local evidence

Run the commands in the scenario document. For the new gateway test, explicitly
configure the authenticated paper gateway variables, then run:

```sh
TRUST_IBKR_LIVE_E2E=1 cargo test --locked -p trust-cli \
  --test integration_test_ibkr_paper_cli -- --include-ignored --test-threads=1 --nocapture
```

This test uses no broker login password, creates its own temporary ledger, and
does not submit orders. Do not run the separately gated order-mutation tests by accident.

Local evidence (not portable repository artifacts):

- `/tmp/trust-feature-suite.log`
- `/tmp/trust-feature-final-main.log`
- `/tmp/trust-feature-no-defaults.log`
- `/tmp/trust-feature-paper-cli.log`
- `/tmp/trust-feature-dispatch-matrix.log`
- `/tmp/trust-ibkr-feature-tests.log`
- `/tmp/trust-feature-snapshots.log`
- `/tmp/trust-feature-clippy.log`
- `/tmp/trust-feature-latest-clippy.log`
- `/tmp/trust-feature-isolated-clippy.log`
- `/tmp/trust-feature-isolated-suite.log`
- `/tmp/trust-feature-autoreview-isolated.json`

Review command: `autoreview --mode local` in the isolated worktree, with a prompt
scoping review to this feature patch. The first main-tree review refused to run
because sources changed during bundle creation. The isolated review returned no
actionable findings at its P0 reporting threshold.
