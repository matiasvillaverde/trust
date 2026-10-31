use crate::error::{Result, ServiceError};
use serde::Serialize;
use serde_json::{json, Map, Value};
use std::sync::LazyLock;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[allow(clippy::enum_variant_names)]
pub enum Operation {
    AccountReadiness,
    AccountRefresh,
    SafetyPolicyGet,
    SafetyPolicySet,
    AccountHalt,
    AccountResume,
    MandateGet,
    MandateSet,
    EquityCashFlow,
    EquityReconcile,
    OperationResolve,
    Status,
    Policy,
    AccountsList,
    AccountGet,
    AccountCreate,
    AccountDelete,
    Balances,
    Transfer,
    Transactions,
    Deposit,
    Withdraw,
    Rules,
    RuleCreate,
    RuleRemove,
    Instruments,
    InstrumentCreate,
    InstrumentUpdate,
    InstrumentStats,
    InstrumentResolve,
    Trades,
    TradeGet,
    TradeCreate,
    TradeFund,
    TradeSubmit,
    TradeCancel,
    TradeClose,
    TradeSync,
    Reconcile,
    StopModify,
    TargetModify,
    ManualFill,
    ManualStop,
    ManualTarget,
    SizePreview,
    RiskCheck,
    Hypothesis,
    BrokerPreview,
    Executions,
    TradeAudit,
    Events,
    EventCreate,
    Mistakes,
    MistakeCreate,
    Reports,
    Metrics,
    GradeGet,
    GradeCompute,
    GradeRecord,
    GradeSummary,
    LevelGet,
    LevelHistory,
    LevelRules,
    LevelRulesSet,
    LevelEvaluate,
    LevelProgress,
    DistributionRules,
    DistributionConfigure,
    DistributionExecute,
    DistributionHistory,
    AdvisorStatus,
    AdvisorCheck,
    AdvisorConfigure,
    AdvisorHistory,
    Correlation,
    Regime,
    Catalysts,
    SessionList,
    SessionOpen,
    SessionClose,
    MarketSnapshot,
    MarketBars,
    MarketQuote,
    MarketTrade,
    MarketSession,
    MarketStream,
    OperationGet,
    AuditList,
    AuditVerify,
    BackupExport,
    BackupImport,
    ConnectionStatus,
    SkillsList,
    SkillRead,
}

#[derive(Clone, Debug, Serialize)]
pub struct ToolSpec {
    #[serde(skip)]
    pub operation: Operation,
    pub name: &'static str,
    pub description: &'static str,
    pub scope: &'static str,
    pub owner_only: bool,
    pub mutation: bool,
    pub account_required: bool,
    pub input_schema: Value,
}

impl ToolSpec {
    /// Journal entries cannot change cash, orders or enforced risk policy.
    /// Their uncertain completion remains inspectable without halting trading.
    pub fn affects_financial_state(&self) -> bool {
        self.mutation && self.scope != "trust:journal" && self.operation != Operation::BackupExport
    }
}

type Field = (&'static str, Value, bool);
fn typed<T: schemars::JsonSchema>(name: &'static str, required: bool) -> Field {
    let generator = schemars::generate::SchemaSettings::draft2020_12()
        .with(|settings| {
            settings.inline_subschemas = true;
            settings.contract = schemars::generate::Contract::Serialize;
        })
        .into_generator();
    let mut schema = json!(generator.into_root_schema_for::<T>());
    exact_decimal_schema(&mut schema);
    (name, schema, required)
}
// Schemars' serialization contract identifies Decimal by this pattern. Add our
// exact, bounded monetary format so nested policy values get the same checks as money().
fn exact_decimal_schema(schema: &mut Value) {
    match schema {
        Value::Object(object) => {
            if object.get("pattern").and_then(Value::as_str) == Some(r"^-?\d+(\.\d+)?$") {
                object.insert("format".into(), json!("decimal"));
                object.insert("maxLength".into(), json!(64));
            }
            for value in object.values_mut() {
                exact_decimal_schema(value);
            }
        }
        Value::Array(items) => {
            for item in items {
                exact_decimal_schema(item);
            }
        }
        _ => {}
    }
}
fn text(name: &'static str, required: bool) -> Field {
    (
        name,
        json!({"type":"string","minLength":1,"maxLength":2000}),
        required,
    )
}
fn identifier(name: &'static str, required: bool) -> Field {
    (name, json!({"type":"string","format":"uuid"}), required)
}

fn percentage_points(name: &'static str) -> (&'static str, Value, bool) {
    (
        name,
        json!({"type":"string", "maxLength":64, "description":"Exact decimal percentage points: 1 means 1%, range 0..100. Tax plus earnings must be <=100."}),
        false,
    )
}

fn money(name: &'static str, required: bool) -> Field {
    (
        name,
        json!({"type":"string","format":"decimal","description":"Exact decimal string; never a JSON floating-point number.","maxLength":64}),
        required,
    )
}
fn choice(name: &'static str, values: &[&str], required: bool) -> Field {
    (name, json!({"type":"string","enum":values}), required)
}
fn number(name: &'static str, min: u64, max: u64, required: bool) -> Field {
    (
        name,
        json!({"type":"integer","minimum":min,"maximum":max}),
        required,
    )
}
fn flag(name: &'static str) -> Field {
    (name, json!({"type":"boolean"}), false)
}
fn strings(name: &'static str, required: bool) -> Field {
    (
        name,
        json!({"type":"array","maxItems":64,"items":{"type":"string","minLength":1,"maxLength":200}}),
        required,
    )
}
fn period() -> Vec<Field> {
    vec![
        text("from", false),
        text("to", false),
        number("days", 1, 36500, false),
    ]
}
fn currency() -> Field {
    choice("currency", &["usd", "eur", "btc"], false)
}
fn trade() -> Vec<Field> {
    vec![identifier("trade_id", true)]
}

fn spec(
    operation: Operation,
    name: &'static str,
    description: &'static str,
    scope: &'static str,
    mutation: bool,
    account: bool,
    mut fields: Vec<Field>,
) -> ToolSpec {
    if account {
        fields.insert(0, identifier("account_id", true));
    }
    if mutation {
        fields.push(("idempotency_key", json!({"type":"string","minLength":8,"maxLength":128,"description":"Stable unique key for this exact intent. Reuse after a timeout; never generate a new key to blindly repeat an uncertain mutation."}), true));
    }
    let required: Vec<_> = fields
        .iter()
        .filter(|(_, _, required)| *required)
        .map(|(name, _, _)| *name)
        .collect();
    let properties: Map<String, Value> = fields
        .into_iter()
        .map(|(name, value, _)| (name.to_owned(), value))
        .collect();
    ToolSpec {
        operation,
        name,
        description,
        scope,
        owner_only: crate::auth::owner_scope(scope),
        mutation,
        account_required: account,
        input_schema: json!({"type":"object","properties":properties,"required":required,"additionalProperties":false}),
    }
}

/// One authoritative catalogue for MCP, HTTP, CLI discovery, and scope enforcement.
/// Immutable schemas are built once. Identity filtering still runs on every request.
pub fn tools() -> &'static [ToolSpec] {
    static TOOLS: LazyLock<Vec<ToolSpec>> = LazyLock::new(build_tools);
    &TOOLS
}

fn build_tools() -> Vec<ToolSpec> {
    use Operation::*;
    let mut result = vec![
        spec(AccountReadiness,"account_readiness","Start here for an account: current safety blockers, broker observation age, halt, policy and mandate status. This read does not refresh broker evidence.","trust:read",false,true,vec![]),
        spec(AccountRefresh,"account_refresh","Fetch and persist a complete broker-account snapshot and equity limits. Failures latch a durable halt; refresh never resumes trading.","trust:reconcile",true,true,vec![]),
        spec(SafetyPolicyGet,"account_safety_policy_get","Read owner-configured whole-broker-account limits in its explicit base currency.","trust:read",false,true,vec![]),
        spec(SafetyPolicySet,"account_safety_policy_set","Set whole-broker-account limits and prospective risk controls. Does not reset equity, usage, reservations or a halt. All linked accounts require owner authority.","trust:policy",true,true,vec![choice("currency",&["USD","EUR","BTC"],true),money("max_gross_notional",true),money("max_order_notional",true),money("max_daily_loss",true),money("max_drawdown",true),number("max_snapshot_age_seconds",1,120,true),typed::<model::portfolio_risk::PortfolioRiskPolicy>("controls",true)]),
        spec(AccountHalt,"account_halt","Persistently stop new exposure for the broker account. Existing protection, exits and reconciliation remain available.","trust:trade",true,true,vec![text("reason",true)]),
        spec(AccountResume,"account_resume","Resume only after fresh, complete broker evidence, satisfied equity/exposure limits and resolution of every uncertain operation. Owner MFA required.","trust:policy",true,true,vec![text("reason",true)]),
        spec(MandateGet,"agent_mandate_get","Read this agent's limits, or another agent's mandate when called by the owner.","trust:read",false,true,vec![text("subject",false)]),
        spec(MandateSet,"agent_mandate_set","Set an agent's instruments, notional, open trades, direction, expiry and daily loss/turnover/re-entry budgets. Changing policy never resets consumed budgets.","trust:policy",true,true,vec![text("subject",true),strings("instrument_ids",true),money("max_order_notional",true),money("max_total_notional",true),number("max_open_trades",1,1000,true),("allow_short",json!({"type":"boolean"}),true),text("expires_at",true),typed::<model::portfolio_risk::AgentRiskBudget>("risk_budget",true)]),
        spec(EquityCashFlow,"account_cashflow_record","Record an independently evidenced external broker cash flow to adjust equity baselines without resetting trading performance. Does not transfer cash or clear a halt.","trust:policy",true,true,vec![text("reference",true),money("amount",true)]),
        spec(EquityReconcile,"account_equity_reconcile","Owner-verifies a broker P&L counter reset against independent statement evidence and the net external cash flow since the last accepted observation. Preserves measured losses and drawdown; never clears a halt.","trust:policy",true,true,vec![text("reference",true),money("cash_flow_amount",true)]),
        spec(OperationResolve,"operation_resolve","Record owner reconciliation evidence for an uncertain operation after matching persisted trades against a fresh broker snapshot. Never replays the original mutation.","trust:policy",true,true,vec![identifier("operation_id",true),text("evidence",true)]),
        spec(Status,"service_status","Start here: service mode, capabilities, account access, and suggested workflow.","trust:read",false,false,vec![]),
        spec(Policy,"policy_get","Explain Trust's risk, permission, money, and execution contracts.","trust:read",false,false,vec![]),
        spec(AccountsList,"accounts_list","List only accounts this identity may access.","trust:read",false,false,vec![]),
        spec(AccountGet,"account_get","Read one permitted account and its current balances.","trust:read",false,true,vec![]),
        spec(AccountCreate,"account_create","Create a local account profile. Does not open a brokerage account; operator grant required.","trust:admin",true,false,vec![text("name",true),text("description",true),choice("environment",&["paper","live"],true),choice("broker",&["ibkr","alpaca"],true),text("broker_account_id",false),choice("account_type",&["primary","earnings","tax_reserve","reinvestment","insurance"],false),identifier("parent_account_id",false),percentage_points("taxes_percentage"),percentage_points("earnings_percentage")]),
        spec(AccountDelete,"account_delete","Soft-delete an empty account; active trades and children still block deletion.","trust:admin",true,true,vec![flag("force")]),
        spec(Balances,"balances_get","Read the local ledger balances; these are not a live broker balance snapshot.","trust:read",false,true,vec![]),
        spec(Transfer,"capital_transfer","Transfer ledger capital within an account hierarchy; requires access to both accounts.","trust:policy",true,true,vec![identifier("to_account_id",true),money("amount",true),currency(),text("reason",true)]),
        spec(Transactions,"transactions_list","Read the account's ledger transactions.","trust:read",false,true,period()),
        spec(Deposit,"capital_deposit","Record a ledger deposit. Does not transfer money at the broker.","trust:policy",true,true,vec![money("amount",true),currency()]),
        spec(Withdraw,"capital_withdraw","Record a ledger withdrawal subject to available capital.","trust:policy",true,true,vec![money("amount",true),currency()]),
        spec(Rules,"risk_rules_list","Read enforced risk rules for the account.","trust:read",false,true,vec![]),
        spec(RuleCreate,"risk_rule_create","Configure an enforced per-trade or monthly risk percentage; policy permission required.","trust:policy",true,true,vec![choice("kind",&["per_trade","per_month"],true),money("percentage",true),choice("priority",&["advice","warning","error"],true),text("description",true)]),
        spec(RuleRemove,"risk_rule_remove","Deactivate one of this account's risk rules; policy permission required.","trust:policy",true,true,vec![identifier("rule_id",true)]),
        spec(Instruments,"instruments_list","Search normalized instrument metadata. Use instrument IDs when preparing trades.","trust:read",false,false,vec![text("symbol",false),choice("category",&["stock","etf","bond","crypto","fiat"],false),text("broker",false)]),
        spec(InstrumentCreate,"instrument_create","Register an instrument in the shared catalogue; operator grant required.","trust:admin",true,false,vec![text("symbol",true),text("isin",false),choice("category",&["stock","etf","bond","crypto","fiat"],true),choice("broker",&["ibkr","alpaca"],true)]),
        spec(InstrumentUpdate,"instrument_update","Update normalized instrument metadata, including fixed-income terms.","trust:admin",true,false,vec![identifier("instrument_id",true),text("broker_asset_id",false),text("exchange",false),text("broker_asset_class",false),text("broker_asset_status",false),flag("tradable"),flag("marginable"),flag("shortable"),flag("easy_to_borrow"),flag("fractionable"),money("face_value",false),money("annual_coupon_rate_pct",false),text("maturity_date",false),number("coupon_frequency_per_year",1,365,false)]),
        spec(InstrumentResolve,"instrument_resolve","Resolve a symbol through the account's broker and persist its native ID and capabilities. All-account administration required.","trust:admin",true,false,vec![identifier("account_id",true),text("symbol",true),choice("category",&["stock","etf","bond","crypto","fiat"],true)]),
        spec(InstrumentStats,"instruments_stats","Summarize instrument inventory and missing bond terms.","trust:read",false,false,vec![]),
        spec(Trades,"trades_list","List this account's trades with optional lifecycle and symbol filters.","trust:read",false,true,vec![text("status",false),text("symbol",false),flag("open_only")]),
        spec(TradeGet,"trade_get","Read persisted trade, bracket orders and accounting. Submitted is not filled.","trust:read",false,true,trade()),
        spec(TradeCreate,"trade_prepare","Create a draft trade with exact instrument, quantity, entry, stop and target. Does not reserve or submit capital.","trust:trade",true,true,vec![identifier("instrument_id",true),choice("side",&["long","short"],true),money("quantity",true),money("entry",true),money("stop",true),money("target",true),currency(),choice("stop_type",&["stop","stop_limit"],false),text("thesis",false),text("sector",false),text("asset_class",false),text("context",false)]),
        spec(TradeFund,"trade_fund","Validate Trust risk rules and reserve capital for a persisted draft. Never submits an order.","trust:trade",true,true,trade()),
        spec(TradeSubmit,"trade_submit","Submit a funded trade's exact persisted bracket. On uncertainty inspect operation and reconcile; never resubmit blindly.","trust:trade",true,true,trade()),
        spec(TradeCancel,"trade_cancel","Cancel a funded, submitted or partially filled entry while preserving protection for filled exposure. Confirm cancellation and zero exposure before assuming capital is released.","trust:trade",true,true,trade()),
        spec(TradeClose,"trade_close","Request broker exit of a Filled trade; PartiallyFilled is not eligible and there is no quantity override. Inspect cancellation/reconciliation actions for partial entries. Reconcile to confirm exit fills and released capital.","trust:trade",true,true,trade()),
        spec(TradeSync,"trade_reconcile","Reconcile one trade with broker orders, executions and fees; may update the ledger. Success with empty orders does not settle an unknown submission or prove zero exposure.","trust:reconcile",true,true,trade()),
        spec(Reconcile,"account_reconcile","Reconcile a bounded page of active trades; inspect errors and next_after_trade_id.","trust:reconcile",true,true,vec![identifier("after_trade_id",false),number("batch_size",1,5,false)]),
        spec(StopModify,"trade_modify_stop","Adjust the protective stop through Trust's no-widening validation.","trust:trade",true,true,vec![identifier("trade_id",true),money("price",true)]),
        spec(TargetModify,"trade_modify_target","Modify a target while preserving durable intent and broker reconciliation.","trust:trade",true,true,vec![identifier("trade_id",true),money("price",true)]),
        spec(ManualFill,"trade_record_fill","Record a manual fill in the ledger. Manual accounting authority required; no broker order is sent.","trust:manual",true,true,vec![identifier("trade_id",true),money("fee",true),money("price",true)]),
        spec(ManualStop,"trade_record_stop","Record a manual stop outcome and accounting; manual accounting authority required.","trust:manual",true,true,vec![identifier("trade_id",true),money("fee",true),money("price",true)]),
        spec(ManualTarget,"trade_record_target","Record a manual target outcome and accounting; manual accounting authority required.","trust:manual",true,true,vec![identifier("trade_id",true),money("fee",true),money("price",true)]),
        spec(SizePreview,"risk_preview","Calculate permitted size from local ledger, risk rules and account level. Broker capacity is a separate preview.","trust:preview",false,true,vec![money("entry",true),money("stop",true),currency()]),
        spec(RiskCheck,"risk_check","Read-only, draft-free admission preview with fresh broker, margin and market evidence. Returns a conservative quantity within the requested ceiling, binding blockers and modeled loss; never reserves or submits.","trust:preview",false,true,vec![identifier("instrument_id",true),choice("side",&["long","short"],true),money("quantity",true),money("entry",true),money("stop",true),money("target",true),currency()]),
        spec(Hypothesis,"trade_hypothesis","Calculate planned stop risk, reward and risk/reward for an exact proposed quantity.","trust:preview",false,true,vec![money("entry",true),money("stop",true),money("target",true),money("quantity",true),currency()]),
        spec(BrokerPreview,"broker_preview","Preview a persisted bracket at IBKR without submitting it; returns broker commission/margin estimates and warnings.","trust:preview",false,true,trade()),
        spec(Executions,"executions_list","Read execution-level fill records for a trade.","trust:read",false,true,trade()),
        spec(TradeAudit,"trade_audit","Read and verify the trade's financial audit chain.","trust:read",false,true,trade()),
        spec(Events,"trade_events_list","Read recorded catalysts and other trade events.","trust:read",false,true,trade()),
        spec(EventCreate,"trade_event_record","Record a structured event associated with a trade.","trust:journal",true,true,vec![identifier("trade_id",true),text("event_type",true),text("severity",true),text("event_date",true),text("notes",false)]),
        spec(Mistakes,"trade_mistakes_list","Read post-trade process and bias observations.","trust:read",false,true,trade()),
        spec(MistakeCreate,"trade_mistake_record","Record a structured trade autopsy and lesson.","trust:journal",true,true,vec![identifier("trade_id",true),text("lesson",true),text("error_type",true),strings("tendencies",true),flag("lollapalooza"),money("counterfactual_r",false),text("violated_rule",false)]),
        spec(Reports,"report_get","Compute account reports using Trust's financial calculators; errors remain explicit.","trust:read",false,true,vec![choice("kind",&["summary","performance","drawdown","risk","concentration","metrics","attribution","benchmark","timeline","bias","wash_sales","tax"],true),number("days",1,36500,false),text("from",false),text("to",false),choice("by",&["symbol","sector","asset_class"],false),text("benchmark",false)]),
        spec(Metrics,"metrics_get","Compute advanced/comparison metrics or fixed-income analytics.","trust:read",false,true,vec![choice("kind",&["advanced","compare","bond"],true),number("days",1,36500,false),identifier("other_account_id",false),money("face_value",false),money("price",false),money("annual_coupon_rate_pct",false),money("years_to_maturity",false),number("quantity",1,100000000,false)]),
        spec(GradeGet,"grade_get","Read the latest recorded grade for a trade.","trust:read",false,true,trade()),
        spec(GradeCompute,"grade_preview","Calculate a grade without persisting it.","trust:preview",false,true,trade()),
        spec(GradeRecord,"grade_record","Calculate and append a trade grade snapshot.","trust:journal",true,true,trade()),
        spec(GradeSummary,"grades_list","Read recent trade grade snapshots for the account.","trust:read",false,true,vec![number("days",1,36500,false)]),
        spec(LevelGet,"level_get","Read the account's current risk level and multiplier.","trust:read",false,true,vec![]),
        spec(LevelHistory,"level_history","Read the account's audited level changes.","trust:read",false,true,vec![number("days",1,36500,false)]),
        spec(LevelRules,"level_policy_get","Read account leveling policy and supported transitions.","trust:read",false,true,vec![]),
        spec(LevelRulesSet,"level_policy_set","Change one named leveling threshold; requires policy authority.","trust:policy",true,true,vec![text("key",true),text("value",true)]),
        spec(LevelEvaluate,"level_evaluate","Evaluate policy against server-computed performance, optionally apply or explicitly change a level.","trust:policy",true,true,vec![flag("apply"),number("target_level",0,4,false),text("reason",false)]),
        spec(LevelProgress,"level_progress","Explain progress toward the next policy-driven level transition.","trust:read",false,true,vec![]),
        spec(DistributionRules,"distribution_policy_get","Read profit allocation rules, excluding password hashes.","trust:read",false,true,vec![]),
        spec(DistributionConfigure,"distribution_policy_set","Configure earnings/tax/reinvestment/insurance allocation. Uses an operator-mounted secret, never an agent-supplied password.","trust:policy",true,true,vec![money("earnings",true),money("tax",true),money("reinvestment",true),money("insurance",true),money("minimum",true)]),
        spec(DistributionExecute,"distribution_execute","Distribute available realized profit into the account's child accounts.","trust:policy",true,true,vec![money("amount",true),currency()]),
        spec(DistributionHistory,"distribution_history","Read realized-profit distributions and their accounting legs.","trust:read",false,true,vec![]),
        spec(AdvisorStatus,"advisor_status","Read portfolio concentration status and advisory thresholds.","trust:read",false,true,vec![]),
        spec(AdvisorCheck,"advisor_check","Evaluate proposed exposure against configured concentration thresholds.","trust:preview",false,true,vec![text("symbol",true),money("entry",true),money("quantity",true),text("sector",false),text("asset_class",false)]),
        spec(AdvisorConfigure,"advisor_configure","Configure concentration advisory thresholds.","trust:policy",true,true,vec![money("sector_limit_pct",true),money("asset_class_limit_pct",true),money("single_position_limit_pct",true)]),
        spec(AdvisorHistory,"advisor_history","Read advisory observations from this service process; financial audit is durable separately.","trust:read",false,true,vec![number("days",1,36500,false)]),
        spec(Correlation,"advisor_correlation","Analyze candidate correlation against open positions using broker bars.","trust:preview",false,true,vec![text("symbol",true)]),
        spec(Regime,"advisor_regime","Evaluate market regime from broker historical bars.","trust:preview",false,true,vec![text("symbol",true)]),
        spec(Catalysts,"trade_catalysts_scan","Scan configured calendar providers and record trade catalysts. Provider secrets are configured by the operator.","trust:journal",true,true,trade()),
        spec(SessionList,"sessions_list","Read trading plans and reviews within a time window.","trust:read",false,true,period()),
        spec(SessionOpen,"session_open","Record the trading session plan before taking new risk. Setup names must be nonblank, unique ignoring case and contain no commas; hypothesis is at most 500 characters.","trust:journal",true,true,vec![choice("regime",&["calm","normal","elevated"],true),("setups",json!({"type":"array","minItems":1,"maxItems":64,"items":{"type":"string","minLength":1,"maxLength":200,"pattern":"^[^,]+$"}}),true),number("max_positions",0,1000,true),("hypothesis",json!({"type":"string","minLength":1,"maxLength":500}),true),text("success_criteria",true),text("failure_criteria",true)]),
        spec(SessionClose,"session_close","Close the account's active session with adherence notes and a grade.","trust:journal",true,true,vec![text("notes",true),text("grade",true)]),
        spec(MarketSnapshot,"market_snapshot","Read broker market snapshot with source and observation time.","trust:read",false,true,vec![text("symbol",true)]),
        spec(MarketBars,"market_bars","Read bounded historical bars from the configured broker.","trust:read",false,true,vec![text("symbol",true),text("from",true),text("to",true),choice("timeframe",&["1min","1h","1d"],true)]),
        spec(MarketQuote,"market_quote","Read latest broker bid/ask quote; unavailable data is an error.","trust:read",false,true,vec![text("symbol",true)]),
        spec(MarketTrade,"market_trade","Read the latest broker trade tick.","trust:read",false,true,vec![text("symbol",true)]),
        spec(MarketSession,"market_session","Read market snapshot session context without assuming exchange-wide calendars.","trust:read",false,true,vec![text("symbol",true)]),
        spec(MarketStream,"market_stream","Read a finite market-event batch. This tool does not keep the agent connection alive indefinitely.","trust:read",false,true,vec![strings("symbols",true),strings("channels",true),number("max_events",1,100,true),number("timeout_seconds",1,10,true)]),
        spec(OperationGet,"operation_get","Inspect a previously requested mutation by idempotency key. Unknown outcomes require reconciliation, not another submission.","trust:read",false,false,vec![text("key",true)]),
        spec(AuditList,"audit_list","Read service authorization/operation audit entries visible to this identity.","trust:read",false,false,vec![number("after",0,u64::MAX,false),number("limit",1,200,false)]),
        spec(AuditVerify,"audit_verify","Verify the service audit hash chain; operator access required.","trust:admin",false,false,vec![]),
        spec(BackupExport,"backup_export","Create a SQLCipher-encrypted financial backup using the separately mounted backup key. Returns an opaque artifact ID, never a filesystem path.","trust:admin",true,false,vec![]),
        spec(BackupImport,"backup_restore","Restore an encrypted server-owned backup with the mounted backup key into an empty financial database. In-place rollback is an offline operator workflow.","trust:admin",true,false,vec![identifier("backup_id",true)]),
        spec(ConnectionStatus,"connections_status","Report configured broker integrations without exposing credentials.","trust:read",false,false,vec![]),
        spec(SkillsList,"skills_list","Choose a Trust workflow by description; discover its version, entrypoint and supporting files with integrity hashes. Load only the skill/reference needed for the user's task.","trust:read",false,false,vec![]),
        spec(SkillRead,"skill_read","Read a Trust skill or one of its bundled reference files. Omit path for SKILL.md; use an exact relative path from skills_list for supporting guidance. Instructions grant no permissions.","trust:read",false,false,vec![choice("name",&["trust-risk","trust-operations","trust-admin"],true),choice("path",crate::skills::paths(),false)]),
    ];
    for tool in &mut result {
        if !tool.mutation {
            if let Some(properties) = tool
                .input_schema
                .get_mut("properties")
                .and_then(Value::as_object_mut)
            {
                properties
                    .entry("offset")
                    .or_insert(json!({"type":"integer","minimum":0,"maximum":1000000}));
                properties
                    .entry("limit")
                    .or_insert(json!({"type":"integer","minimum":1,"maximum":200}));
            }
        }
    }
    result.sort_by_key(|tool| tool.name);
    result
}

pub fn find(name: &str) -> Result<&'static ToolSpec> {
    tools()
        .binary_search_by_key(&name, |tool| tool.name)
        .ok()
        .and_then(|index| tools().get(index))
        .ok_or_else(|| {
            ServiceError::new(
                "unknown_tool",
                "Unknown Trust tool.",
                "List available tools before calling one.",
            )
        })
}

pub fn validate(tool: &ToolSpec, args: &Value) -> Result<()> {
    let object = args
        .as_object()
        .ok_or_else(|| ServiceError::invalid("Tool arguments must be an object."))?;
    let properties = tool
        .input_schema
        .get("properties")
        .and_then(Value::as_object)
        .ok_or_else(ServiceError::internal)?;
    if let Some(required) = tool.input_schema.get("required").and_then(Value::as_array) {
        for name in required.iter().filter_map(Value::as_str) {
            if !object.contains_key(name) {
                return Err(ServiceError::invalid(format!(
                    "Missing required field: {name}."
                )));
            }
        }
    }
    for (name, value) in object {
        let schema = properties
            .get(name)
            .ok_or_else(|| ServiceError::invalid(format!("Unknown field: {name}.")))?;
        validate_value(name, value, schema)?;
    }
    if tool.operation == Operation::SessionOpen {
        let setups: Vec<String> = args
            .get("setups")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .map(str::to_owned)
            .collect();
        model::format_session_setups(&setups).map_err(|_| {
            ServiceError::invalid(
                "Setup names must be nonblank, unique ignoring case and contain no commas.",
            )
        })?;
    }
    if matches!(
        tool.operation,
        Operation::SessionOpen | Operation::SessionClose
    ) {
        for name in [
            "hypothesis",
            "success_criteria",
            "failure_criteria",
            "grade",
            "notes",
        ] {
            if args
                .get(name)
                .and_then(Value::as_str)
                .is_some_and(|value| value.trim().is_empty())
            {
                return Err(ServiceError::invalid(format!("{name} must not be blank.")));
            }
        }
    }
    Ok(())
}

fn validate_value(name: &str, value: &Value, schema: &Value) -> Result<()> {
    let invalid = || {
        ServiceError::invalid(format!(
            "Invalid value for {name}; consult its input schema."
        ))
    };
    match schema.get("type").and_then(Value::as_str) {
        Some("string") => {
            let text = value.as_str().ok_or_else(invalid)?;
            let length = u64::try_from(text.chars().count()).map_err(|_| invalid())?;
            if length < schema.get("minLength").and_then(Value::as_u64).unwrap_or(0)
                || length
                    > schema
                        .get("maxLength")
                        .and_then(Value::as_u64)
                        .unwrap_or(2000)
                || text.contains('\0')
            {
                return Err(invalid());
            }
            if schema.get("pattern").and_then(Value::as_str) == Some(r"^-?\d+(\.\d+)?$")
                && (text.starts_with('+') || text.trim() != text)
            {
                return Err(invalid());
            }
            match schema.get("format").and_then(Value::as_str) {
                Some("date-time") if chrono::DateTime::parse_from_rfc3339(text).is_err() => {
                    return Err(invalid())
                }
                Some("uuid") if uuid::Uuid::parse_str(text).is_err() => return Err(invalid()),
                Some("decimal")
                    if rust_decimal::Decimal::from_str_exact(text).is_err()
                        || text.contains(['e', 'E']) =>
                {
                    return Err(invalid())
                }
                _ => {}
            }
        }
        Some("integer") => {
            let number = value.as_u64().ok_or_else(invalid)?;
            if number < schema.get("minimum").and_then(Value::as_u64).unwrap_or(0)
                || number
                    > schema
                        .get("maximum")
                        .and_then(Value::as_u64)
                        .unwrap_or(u64::MAX)
            {
                return Err(invalid());
            }
        }
        Some("boolean") if value.is_boolean() => {}
        Some("object") => {
            let object = value.as_object().ok_or_else(invalid)?;
            if object.len() > 1000 {
                return Err(invalid());
            }
            if let Some(required) = schema.get("required").and_then(Value::as_array) {
                if required
                    .iter()
                    .filter_map(Value::as_str)
                    .any(|key| !object.contains_key(key))
                {
                    return Err(invalid());
                }
            }
            for (key, value) in object {
                let child = schema
                    .get("properties")
                    .and_then(|properties| properties.get(key))
                    .or_else(|| {
                        schema
                            .get("additionalProperties")
                            .filter(|value| value.is_object())
                    })
                    .ok_or_else(invalid)?;
                validate_value(name, value, child)?;
            }
        }
        Some("array") => {
            let items = value.as_array().ok_or_else(invalid)?;
            let length = u64::try_from(items.len()).map_err(|_| invalid())?;
            if length > schema.get("maxItems").and_then(Value::as_u64).unwrap_or(64)
                || length < schema.get("minItems").and_then(Value::as_u64).unwrap_or(0)
            {
                return Err(invalid());
            }
            let item_schema = schema.get("items").ok_or_else(invalid)?;
            for item in items {
                validate_value(name, item, item_schema)?;
            }
        }
        _ => return Err(invalid()),
    }
    if schema
        .get("enum")
        .and_then(Value::as_array)
        .is_some_and(|choices| !choices.contains(value))
    {
        return Err(invalid());
    }
    Ok(())
}
