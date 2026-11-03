# Independent IBKR statement reconciliation

Trust can retain an original IBKR **Activity Flex XML** export and compare its
executions, commissions, ending positions and native-currency cash with persisted
Trust evidence. This supplies an independent reporting path alongside Client Portal
Gateway reconciliation. It never books a deposit, invents a fill, sends a broker
order, changes risk policy or automatically resumes trading.

Use a separate owner session with `trust:policy`, fresh human MFA, and access to
**every Trust profile bound to that broker/environment/account**. Trading agents
cannot import or read statements. Statements may contain account-identifying data.
Production storage requires the existing SQLCipher configuration; the original XML
and reconciliation attempts live in the encrypted **control journal**.

## Export the supported query profile

Create an Activity Flex query in IBKR reporting, for one account and complete closed
calendar days. Select XML, `YYYYMMDD` dates, `HHMMSS` times and semicolon date/time
separator. Export **all executions, instruments and cash activity**, without symbol,
strategy or trade-side filters. Retain the broker's original UTF-8 bytes. Identify
the actual report timezone as an IANA name, for example `America/New_York`; Trust
does not infer it from the server timezone or guess during a DST overlap.

Required containers must be present even when empty:

| Section | Required fields / detail |
| --- | --- |
| `FlexStatement` | `accountId`, `fromDate`, `toDate`, `whenGenerated` |
| `CashReport/CashReportCurrency` | Native `currency`, `fromDate`, `toDate`, `startingCash`, `endingCash`, `commissions`, `depositWithdrawals` |
| `Trades/Trade` | `levelOfDetail=EXECUTION`, `assetCategory`, `multiplier`, `transactionType`, `ibExecID`, `conid`, `symbol`, `currency`, `buySell`, signed `quantity`, `tradePrice`, `dateTime`, `ibCommission`, `ibCommissionCurrency`, `taxes`; include `origTradeID` when supplied and `proceeds` |
| `OpenPositions/OpenPosition` | `levelOfDetail=SUMMARY`, `reportDate`, `assetCategory`, `multiplier`, `conid`, `currency`, signed `position` |
| `CashTransactions/CashTransaction` | `transactionID`, `type`, `currency`, signed `amount`, `dateTime` |

The initial adapter supports stocks/ETFs (`STK`, multiplier `1`), USD and EUR kept
separate, ordinary exchange executions, same-currency commission costs, and
`Deposits/Withdrawals` cash flows. Buy quantities are positive; sells are negative
in XML. Commissions are negative costs in XML. `BASE_SUMMARY` is ignored in favor of
native currencies; settled cash is not substituted for trade-date cash.

The report must have been generated after its final day completed in the supplied
timezone. A current-day partial report is rejected. Both ambiguous and nonexistent
local timestamps are rejected. The parser rejects duplicate identities, mixed
accounts, missing sections, incorrect detail levels, invalid numbers, DTD/entity
definitions, processing instructions, non-whitespace element text and oversized XML.

Unsupported assets, corrections/cancellations, rebates, trade taxes, FX, dividends,
interest, corporate actions and other populated financial sections cannot qualify
as matched evidence. Depending on whether their representation can be parsed safely,
they produce an inconclusive report or a definitive import rejection. Do not remove
them from the export to make it pass. A future adapter must account for them explicitly.

## Owner MCP workflow

1. Discover the current tool schemas. Read `account_readiness`, confirm exact broker
   binding and reconcile relevant trades with Client Portal evidence first.
2. Call `broker_statement_import` with `account_id`, the entire original XML string
   in `xml`, its `timezone`, `complete_report: true`, and a stable `idempotency_key`.
   The confirmation attests that this is an original, complete, unfiltered account
   report. No file paths, URLs, Flex tokens or other credentials are accepted.
3. Inspect `data.reconciliation.result.status`, every issue, and cash comparisons.
   Store the statement ID, source SHA-256, attempt ID, period and checked timestamp
   with the acceptance record. A successful tool call can return discrepancies.
4. Use `broker_statements_list` (`limit` up to 20, `next_offset`) and
   `broker_statement_get` for evidence inspection. The latter returns the original
   only when `include_source: true`; avoid copying sensitive XML into ordinary logs.
5. After independently resolving missing broker evidence through existing workflows,
   use `broker_statement_reconcile` with that statement ID and a **new** key. This
   appends an attempt against the current ledger. Replaying an old key returns the
   original result; it does not recheck current data.

For example, after an owner-authorized import, a recheck is:

```json
{"tool":"broker_statement_reconcile","arguments":{"account_id":"11111111-1111-4111-8111-111111111111","statement_id":"22222222-2222-4222-8222-222222222222","idempotency_key":"statement-review-01-recheck"}}
```

The comparison uses a single financial database snapshot covering all linked
profiles. It reconstructs actual cash from external capital receipts, signed
execution notionals and allocated broker fees. It does **not** use available cash,
capital reservations, equity or current balance projections. Booked fee receipts must
agree with allocated broker fee activities; their broker occurrence dates handle
fees booked later. Internal transfers between included profiles net to zero.

Execution IDs, contracts, currencies, side, quantity, price and timestamps (whole
seconds) must agree. Position quantities are exact. Cash and per-trade commission
comparisons allow at most **0.01 of the native currency**, inclusive. This tolerance
is fixed and returned with every result. Opening and closing cash and net external
flows are checked separately. Incomplete pre-Trust history, an incorrect initial
capital date, unallocated/manual fills or missing fee activity must be resolved as
evidence gaps; do not add fictional cash to force equality.

Persisted trade entry/exit quantities must also agree with execution rows. An empty
execution table cannot make recorded filled trades disappear from the comparison.
IBKR's native account-activity executions are used; Trust's derived order-snapshot
deltas describe the same fills and are not counted a second time or accepted as
replacement native execution IDs.

## Halt and recovery semantics

| Result | Meaning and consequence |
| --- | --- |
| `matched` | The supported evidence agrees for this account/period. It grants no trading permission. |
| `discrepancies` | At least one supported comparison disagrees. A durable whole-account halt blocks new exposure. |
| `inconclusive` | Unsupported report content or unavailable/corrupt Trust evidence prevents qualification. The same durable halt applies. |
| Import error | No valid report was retained. No match was established. Inspect the stable error; do not assume reconciliation succeeded. |

Readiness exposes `unreconciled_statement_count`, owner-only IDs and the
`statement_reconciliation_unresolved` blocker. Policy changes, a fresh snapshot and
service restart cannot clear it. Protective orders and permitted lifecycle-valid
exits/reconciliation remain available.

A matched recheck resolves **only its own statement ID**. If IBKR supplies a corrected
original export, the owner can explicitly specify `supersedes_statement_id` on import
or recheck. The previous unresolved report must belong to the same account, date
range and timezone; only a matched replacement resolves it. Merely importing another
matching report leaves previous discrepancies unresolved. Original sources and
attempts are immutable, and supersession stays in the audit record.

Even after all statement issues are resolved, the halt remains. The owner must
resolve other unknown operations, verify current broker protection/exposure and
request `account_resume` under its existing recovery checks. Statement comparison
does not reconcile P&L counter resets or clear live-readiness blockers.

Source, attempt, safety state and their audit links commit atomically. Storage failure
stops the financial worker and preserves operation uncertainty for recovery. Source
bytes, metadata and every attempt are verified during startup and `audit_verify`;
statement reads verify the requested evidence and detect missing retained records.
Protect external audit head receipts as well: a privileged rewrite of the entire
database is outside the guarantee of a local hash chain.

## Bounds, backup and evidence limits

- Maximum source: 262,144 UTF-8 bytes, 10,000 XML nodes and 2,000 rows per section.
- Financial snapshot: at most 64 linked profiles and 20,000 rows each for trades, executions,
  fee activities and relevant cash receipts. Exceeding the evidence limit is
  inconclusive; Trust does not silently truncate it.
- Retain at most 1,000 original statements per broker scope and 256 unresolved IDs.
  Reports are never automatically evicted. When a new source cannot be retained at
  capacity, the worker stops and the operation remains unresolved; operator review
  is required. Existing sources can still be rechecked without adding a new source.
- Results return at most 200 detailed issues plus the full issue count. All detected
  issues still determine the status.
- Back up financial storage **and the control journal** with the worker stopped.
  `backup_export` alone does not include these original reports or durable halts.

An uploaded export is owner-supplied evidence, **not authenticated broker provenance**.
The SHA-256 proves retained-byte integrity, not that IBKR created a complete report.
`source_authenticated` and `permits_trading` are always false. No automatic Flex Web
Service download, report scheduling, freshness requirement, CSV import, ledger repair
or real-money promotion is included. An old matched report says nothing about newer
activity. Fresh whole-account broker controls remain required for admission.

## Verification

```bash
cargo test --locked -p ibkr-broker --test statement_import
cargo test --locked -p core --test statement_reconciliation
cargo test --locked -p trust-service --test statement_workflow -- --test-threads=1
```

Fixtures are explicitly synthetic. These tests establish offline parser, financial,
authorization, encryption, atomicity, restart and halt contracts. They do not prove
compatibility with a real account's query configuration, actual fees, fills or market
behavior. Complete a supervised IBKR paper export and comparison before recording
level-4 evidence in [the broker acceptance procedure](interactive-brokers-e2e.md).
Do not commit original private exports or claim offline tests completed that level.

Source contracts: [IBKR Flex queries](https://www.ibkrguides.com/clientportal/performanceandstatements/flex.htm),
[IBKR Flex reporting fields](https://www.ibkrguides.com/reportingreference/reportguide/financialinformation_fq.htm),
[IBKR cash report](https://www.ibkrguides.com/reportingreference/reportguide/cashreport.htm).
