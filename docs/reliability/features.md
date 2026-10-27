# Trust feature inventory

This inventory describes the implementation under test, including limits that affect what an agent may assume. The canonical service catalogue is `service/src/catalog.rs`; the financial contract is `core::TrustFacade`.

## Service tools (82)

| Tool | Scope | Writes state | Account argument | Behavior |
| --- | --- | --- | --- | --- |
| `account_create` | `trust:admin` | yes | operation-specific | Create a local account profile. Does not open a brokerage account; operator grant required. |
| `account_delete` | `trust:admin` | yes | required | Soft-delete an empty account; active trades and children still block deletion. |
| `account_get` | `trust:read` | no | required | Read one permitted account and its current balances. |
| `account_reconcile` | `trust:reconcile` | yes | required | Reconcile a bounded page of active trades; inspect errors and next_after_trade_id. |
| `accounts_list` | `trust:read` | no | operation-specific | List only accounts this identity may access. |
| `advisor_check` | `trust:preview` | no | required | Evaluate proposed exposure against configured concentration thresholds. |
| `advisor_configure` | `trust:policy` | yes | required | Configure concentration advisory thresholds. |
| `advisor_correlation` | `trust:preview` | no | required | Analyze candidate correlation against open positions using broker bars. |
| `advisor_history` | `trust:read` | no | required | Read advisory observations from this service process; financial audit is durable separately. |
| `advisor_regime` | `trust:preview` | no | required | Evaluate market regime from broker historical bars. |
| `advisor_status` | `trust:read` | no | required | Read portfolio concentration status and advisory thresholds. |
| `audit_list` | `trust:read` | no | operation-specific | Read service authorization/operation audit entries visible to this identity. |
| `audit_verify` | `trust:admin` | no | operation-specific | Verify the service audit hash chain; operator access required. |
| `backup_export` | `trust:admin` | yes | operation-specific | Create a service-owned financial backup artifact. Returns an opaque artifact ID, never a filesystem path. |
| `backup_restore` | `trust:admin` | yes | operation-specific | Import a server-owned backup into an empty financial database. In-place rollback is an offline operator workflow. |
| `balances_get` | `trust:read` | no | required | Read the local ledger balances; these are not a live broker balance snapshot. |
| `broker_preview` | `trust:preview` | no | required | Preview a persisted bracket at IBKR without submitting it; returns broker commission/margin estimates and warnings. |
| `capital_deposit` | `trust:policy` | yes | required | Record a ledger deposit. Does not transfer money at the broker. |
| `capital_transfer` | `trust:policy` | yes | required | Transfer ledger capital within an account hierarchy; requires access to both accounts. |
| `capital_withdraw` | `trust:policy` | yes | required | Record a ledger withdrawal subject to available capital. |
| `connections_status` | `trust:read` | no | operation-specific | Report configured broker integrations without exposing credentials. |
| `distribution_execute` | `trust:policy` | yes | required | Distribute available realized profit into the account's child accounts. |
| `distribution_history` | `trust:read` | no | required | Read realized-profit distributions and their accounting legs. |
| `distribution_policy_get` | `trust:read` | no | required | Read profit allocation rules, excluding password hashes. |
| `distribution_policy_set` | `trust:policy` | yes | required | Configure earnings/tax/reinvestment/insurance allocation. Uses an operator-mounted secret, never an agent-supplied password. |
| `executions_list` | `trust:read` | no | required | Read execution-level fill records for a trade. |
| `grade_get` | `trust:read` | no | required | Read the latest recorded grade for a trade. |
| `grade_preview` | `trust:preview` | no | required | Calculate a grade without persisting it. |
| `grade_record` | `trust:journal` | yes | required | Calculate and append a trade grade snapshot. |
| `grades_list` | `trust:read` | no | required | Read recent trade grade snapshots for the account. |
| `instrument_create` | `trust:admin` | yes | operation-specific | Register an instrument in the shared catalogue; operator grant required. |
| `instrument_resolve` | `trust:admin` | yes | operation-specific | Resolve a symbol through the account's broker and persist its native ID and capabilities. All-account administration required. |
| `instrument_update` | `trust:admin` | yes | operation-specific | Update normalized instrument metadata, including fixed-income terms. |
| `instruments_list` | `trust:read` | no | operation-specific | Search normalized instrument metadata. Use instrument IDs when preparing trades. |
| `instruments_stats` | `trust:read` | no | operation-specific | Summarize instrument inventory and missing bond terms. |
| `level_evaluate` | `trust:policy` | yes | required | Evaluate policy against server-computed performance, optionally apply or explicitly change a level. |
| `level_get` | `trust:read` | no | required | Read the account's current risk level and multiplier. |
| `level_history` | `trust:read` | no | required | Read the account's audited level changes. |
| `level_policy_get` | `trust:read` | no | required | Read account leveling policy and supported transitions. |
| `level_policy_set` | `trust:policy` | yes | required | Change one named leveling threshold; requires policy authority. |
| `level_progress` | `trust:read` | no | required | Explain progress toward the next policy-driven level transition. |
| `market_bars` | `trust:read` | no | required | Read bounded historical bars from the configured broker. |
| `market_quote` | `trust:read` | no | required | Read latest broker bid/ask quote; unavailable data is an error. |
| `market_session` | `trust:read` | no | required | Read market snapshot session context without assuming exchange-wide calendars. |
| `market_snapshot` | `trust:read` | no | required | Read broker market snapshot with source and observation time. |
| `market_stream` | `trust:read` | no | required | Read a finite market-event batch. This tool does not keep the agent connection alive indefinitely. |
| `market_trade` | `trust:read` | no | required | Read the latest broker trade tick. |
| `metrics_get` | `trust:read` | no | required | Compute advanced/comparison metrics or fixed-income analytics. |
| `operation_get` | `trust:read` | no | operation-specific | Inspect a previously requested mutation by idempotency key. Unknown outcomes require reconciliation, not another submission. |
| `policy_get` | `trust:read` | no | operation-specific | Explain Trust's risk, permission, money, and execution contracts. |
| `report_get` | `trust:read` | no | required | Compute account reports using Trust's financial calculators; errors remain explicit. |
| `risk_preview` | `trust:preview` | no | required | Calculate permitted size from local ledger, risk rules and account level. Broker capacity is a separate preview. |
| `risk_rule_create` | `trust:policy` | yes | required | Configure an enforced per-trade or monthly risk percentage; policy permission required. |
| `risk_rule_remove` | `trust:policy` | yes | required | Deactivate one of this account's risk rules; policy permission required. |
| `risk_rules_list` | `trust:read` | no | required | Read enforced risk rules for the account. |
| `service_status` | `trust:read` | no | operation-specific | Start here: service mode, capabilities, account access, and suggested workflow. |
| `session_close` | `trust:journal` | yes | required | Close the account's active session with adherence notes and a grade. |
| `session_open` | `trust:journal` | yes | required | Record the trading session plan before taking new risk. |
| `sessions_list` | `trust:read` | no | required | Read trading plans and reviews within a time window. |
| `skill_read` | `trust:read` | no | operation-specific | Read a bundled Trust workflow skill. |
| `skills_list` | `trust:read` | no | operation-specific | Discover Trust workflow skills and their resource URIs. |
| `trade_audit` | `trust:read` | no | required | Read and verify the trade's financial audit chain. |
| `trade_cancel` | `trust:trade` | yes | required | Cancel a funded or submitted trade. Capital is released only when cancellation and zero exposure are confirmed. |
| `trade_catalysts_scan` | `trust:journal` | yes | required | Scan configured calendar providers and record trade catalysts. Provider secrets are configured by the operator. |
| `trade_close` | `trust:trade` | yes | required | Request broker exit of an open trade. Reconcile to confirm fills and release capital. |
| `trade_event_record` | `trust:journal` | yes | required | Record a structured event associated with a trade. |
| `trade_events_list` | `trust:read` | no | required | Read recorded catalysts and other trade events. |
| `trade_fund` | `trust:trade` | yes | required | Validate Trust risk rules and reserve capital for a persisted draft. Never submits an order. |
| `trade_get` | `trust:read` | no | required | Read persisted trade, bracket orders and accounting. Submitted is not filled. |
| `trade_hypothesis` | `trust:preview` | no | required | Calculate planned stop risk, reward and risk/reward for an exact proposed quantity. |
| `trade_mistake_record` | `trust:journal` | yes | required | Record a structured trade autopsy and lesson. |
| `trade_mistakes_list` | `trust:read` | no | required | Read post-trade process and bias observations. |
| `trade_modify_stop` | `trust:trade` | yes | required | Adjust the protective stop through Trust's no-widening validation. |
| `trade_modify_target` | `trust:trade` | yes | required | Modify a target while preserving durable intent and broker reconciliation. |
| `trade_prepare` | `trust:trade` | yes | required | Create a draft trade with exact instrument, quantity, entry, stop and target. Does not reserve or submit capital. |
| `trade_reconcile` | `trust:reconcile` | yes | required | Reconcile one trade with broker orders, executions and fees; may update the ledger. |
| `trade_record_fill` | `trust:manual` | yes | required | Record a manual fill in the ledger. Manual accounting authority required; no broker order is sent. |
| `trade_record_stop` | `trust:manual` | yes | required | Record a manual stop outcome and accounting; manual accounting authority required. |
| `trade_record_target` | `trust:manual` | yes | required | Record a manual target outcome and accounting; manual accounting authority required. |
| `trade_submit` | `trust:trade` | yes | required | Submit a funded trade's exact persisted bracket. On uncertainty inspect operation and reconcile; never resubmit blindly. |
| `trades_list` | `trust:read` | no | required | List this account's trades with optional lifecycle and symbol filters. |
| `transactions_list` | `trust:read` | no | required | Read the account's ledger transactions. |

An operation-specific account requirement does not mean unrestricted access. Creation, instrument resolution, transfers, comparisons, child-account distribution, backups, and replay have additional authorization checks. The global instrument catalogue requires all-account administration to modify.

## Financial and supporting capabilities

| Area | Features and boundaries |
| --- | --- |
| Accounts and ledger | Paper/live broker-bound profiles; primary, earnings, tax reserve, reinvestment and insurance children; USD/EUR/BTC ledgers; deposits, withdrawals, hierarchical transfers, projections, soft deletion. These are local records, not broker cash transfers. |
| Sizing and risk | Decimal sizing, conservative quantity rounding, available-capital checks, long/short geometry, per-trade and monthly rules, level multipliers, stop tightening, funding gates. Rule percentages retain a legacy f32 model/API boundary; SQLite preserves its exact decimal rendering. Planned stop loss does not guarantee execution price or maximum realized loss. |
| Instruments | Stocks, ETFs, bonds, crypto and fiat metadata; broker native identifiers; fractionability; tradability and borrowing flags; bond face value, coupon, maturity and frequency. A category in the model is not a promise that both brokers support it. |
| Trade lifecycle | Prepare, reserve, submit, fill, partial fill, target/stop exit, cancel, expire, reject; explicit order references; manual reconciliation records; execution and fee identities; broker-operation intents; audit chains; persisted state reloaded before mutation. |
| Portfolio analysis | Summary, performance, realized return, drawdown, planned risk, concentration, advanced statistics, expectancy, attribution, benchmark, timeline, bias, tax/wash-sale records and bond analytics. Mixed currencies must not silently combine. Tax outputs encode application rules, not jurisdiction-complete tax advice. |
| Process controls | Grades, grading history, level progress and thresholds, automatic close evaluation, profit allocation, minimum thresholds and insurance reserve. Advisory history is process-local; ledger and financial history persist. |
| Research and journaling | Concentration checks, correlation, regime analysis, optional catalyst providers, event records, mistakes, biases, session plans and reviews. Advisory output does not replace enforced funding rules. |
| Broker integration | IBKR Client Portal account/environment verification, contract lookup, bracket/exit/modify/cancel, what-if preview, history and finite market events; Alpaca account-bound credentials, supported assets, order/execution/fee mapping. Unsupported/missing data must remain explicit. |
| Service reliability | One financial worker, bounded request queue, per-subject idempotency, durable intent/result journal, unknown-outcome recovery, all-account reauthorization for replay, file exclusivity, deadline cancellation before dispatch, readiness/metrics outside the queue, graceful shutdown, reconciliation pages and periodic reconciliation. |
| Security and transport | OIDC RS256 bearer verification, issuer/audience/lifetime/grants, scopes, account allowlists, separate live gates, revocation, pinned JWKS, Host/Origin checks, size/rate/concurrency limits, redacted structured logs, stateless Streamable HTTP MCP and HTTP tool API. |
| Agent UX | Tool discovery with schemas and scopes; exact decimal strings; error codes and next actions; operation inspection; pagination; three workflow skills as resources/prompts/tools; thin CLI using the same service. |
| Persistence and recovery | SQLite financial records, migrations and FK checks, savepoints and rollback, separate operation journal, hash-chain verification, strict backup validation, server-owned backup IDs, empty-target restore. Both databases require coordinated offline backup. |
| Packaging | Locked Rust dependencies, credential backends, non-root container, read-only root filesystem, mounted private state/secrets, health probes, CI formatting/lint/test/security/container gates. |
| Legacy and scaffolding | Optional legacy CLI and its JSON contract remain testable. broker-sync has actor/state/message and jitter tests, but no production websocket reconciliation connection. Unreferenced `core/src/integration_tests.rs` and root standalone Rust files are not executed by cargo merely because they contain tests. |

## Verification index

See [edge cases](edge-cases.md) for the boundary matrix and [verification](verification.md) for commands, results, known limits, and regression evidence. Tests use isolated SQLite and mock broker responses. No ignored live-broker test is part of the automated gate.

Operational HTTP routes include unauthenticated `/healthz` and `/readyz`, plus `/metrics` restricted to an all-accounts administrator. Audit v2 records affected accounts, transport/source, timing, failures, replays and crash recovery while retaining existing hash verification. See [the operator guide](../service.md#audit-logging-and-operations).
