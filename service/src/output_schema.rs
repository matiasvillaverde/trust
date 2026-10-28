//! Output contracts share model definitions with serialization, including decimal strings.
use crate::catalog::{Operation, ToolSpec};
use schemars::{
    generate::{Contract, SchemaSettings},
    JsonSchema, SchemaGenerator,
};
use serde_json::{json, Map, Value};
use std::{
    collections::BTreeMap,
    sync::{Arc, OnceLock},
};

fn schema<T: JsonSchema>(generator: &mut SchemaGenerator) -> Value {
    generator.subschema_for::<T>().to_value()
}
fn string() -> Value {
    json!({"type":"string"})
}
fn boolean() -> Value {
    json!({"type":"boolean"})
}
fn integer() -> Value {
    json!({"type":"integer","minimum":0})
}
fn uuid() -> Value {
    json!({"type":"string","format":"uuid"})
}
fn timestamp() -> Value {
    json!({"type":"string","format":"date-time"})
}
fn decimal() -> Value {
    json!({"type":"string","pattern":"^-?[0-9]+(\\.[0-9]+)?$","description":"Exact decimal; money is in the indicated currency's major units, quantities in instrument units."})
}
fn nullable(value: Value) -> Value {
    json!({"anyOf":[value,{"type":"null"}]})
}
fn array(value: Value) -> Value {
    json!({"type":"array","items":value})
}
fn union(values: Vec<Value>) -> Value {
    json!({"anyOf":values})
}
fn object(fields: &[(&str, Value)]) -> Value {
    let properties: Map<String, Value> = fields
        .iter()
        .map(|(name, value)| ((*name).to_owned(), value.clone()))
        .collect();
    json!({"type":"object","additionalProperties":false,"required":fields.iter().map(|(name,_)|name).collect::<Vec<_>>(),"properties":properties})
}
fn page(item: Value) -> Value {
    object(&[
        ("items", array(item)),
        ("total", integer()),
        ("next_offset", nullable(integer())),
    ])
}
fn sync(generator: &mut SchemaGenerator) -> Value {
    object(&[
        ("trade_id", uuid()),
        ("status", schema::<model::Status>(generator)),
        ("orders", schema::<Vec<model::Order>>(generator)),
    ])
}
fn metrics(generator: &mut SchemaGenerator) -> Value {
    use trust_core::calculators_advanced_metrics::*;
    let mut fields = vec![
        ("trade_count", integer()),
        ("risk_free_rate", decimal()),
        ("execution", schema::<ExecutionQualityMetrics>(generator)),
        ("journal", schema::<JournalQualityMetrics>(generator)),
        ("streaks", schema::<StreakMetrics>(generator)),
        ("exposure", schema::<ExposureMetrics>(generator)),
    ];
    for name in ["gross_profit", "gross_loss", "net_profit", "win_rate"] {
        fields.push((name, decimal()));
    }
    for name in [
        "profit_factor",
        "expectancy",
        "average_r_multiple",
        "average_trade_pnl",
        "median_trade_pnl",
        "payoff_ratio",
        "sharpe",
        "sortino",
        "value_at_risk_95",
        "expected_shortfall_95",
        "kelly",
    ] {
        fields.push((name, nullable(decimal())));
    }
    object(&fields)
}
fn reports(generator: &mut SchemaGenerator) -> Value {
    use trust_core::{
        calculators_concentration::*,
        calculators_drawdown::*,
        calculators_performance::*,
        calculators_risk::*,
        services::{tax::TaxReport, wash_sale::WashSaleReport},
    };
    union(vec![
        schema::<PerformanceStats>(generator),
        schema::<DrawdownMetrics>(generator),
        schema::<RealizedEquityCurve>(generator),
        schema::<BiasAggregation>(generator),
        page(schema::<ConcentrationGroup>(generator)),
        metrics(generator),
        object(&[
            ("positions", schema::<Vec<OpenPosition>>(generator)),
            ("rules", schema::<Vec<model::Rule>>(generator)),
        ]),
        object(&[
            ("account", schema::<model::Account>(generator)),
            ("balances", schema::<Vec<model::AccountBalance>>(generator)),
            ("performance", schema::<PerformanceStats>(generator)),
            ("risk", schema::<Vec<OpenPosition>>(generator)),
            (
                "concentration",
                schema::<Vec<ConcentrationGroup>>(generator),
            ),
        ]),
        object(&[
            ("by", string()),
            (
                "groups",
                array(object(&[
                    ("key", string()),
                    ("trades", integer()),
                    ("realized_pnl", decimal()),
                ])),
            ),
        ]),
        object(&[
            ("benchmark", string()),
            ("benchmark_return_fraction", decimal()),
            ("first_bar", timestamp()),
            ("last_bar", timestamp()),
            ("realized_pnl", decimal()),
            ("performance", schema::<PerformanceStats>(generator)),
        ]),
        object(&[
            ("period", json!({"const":"full_ledger"})),
            ("report", schema::<TaxReport>(generator)),
        ]),
        object(&[
            ("period", json!({"const":"full_ledger"})),
            ("report", schema::<WashSaleReport>(generator)),
        ]),
    ])
}
fn readiness(generator: &mut SchemaGenerator) -> Value {
    use model::account_safety::*;
    object(&[
        ("account_id", uuid()),
        ("broker", schema::<model::BrokerKind>(generator)),
        ("environment", schema::<model::Environment>(generator)),
        ("ready_for_new_risk", boolean()),
        ("blockers", array(string())),
        ("halted", boolean()),
        ("halted_since", nullable(timestamp())),
        ("policy_configured", boolean()),
        ("observed_at", nullable(timestamp())),
        ("received_at", nullable(timestamp())),
        ("snapshot_age_seconds", nullable(json!({"type":"integer"}))),
        ("snapshot_complete", boolean()),
        ("broker_connected", boolean()),
        ("unresolved_operation_count", integer()),
        ("unresolved_operation_ids", nullable(array(uuid()))),
        (
            "snapshot",
            schema::<Option<BrokerAccountSnapshot>>(generator),
        ),
        ("mandate", schema::<Option<AgentMandate>>(generator)),
        (
            "constraints",
            schema::<Option<trust_core::account_safety::AdmissionCapacity>>(generator),
        ),
        (
            "equity_baseline",
            schema::<Option<EquityBaseline>>(generator),
        ),
        ("risk_reducing_actions_remain_available", boolean()),
        (
            "portfolio_risk",
            schema::<Option<trust_core::portfolio_risk::PortfolioRiskReport>>(generator),
        ),
    ])
}

fn data(op: Operation, generator: &mut SchemaGenerator) -> Value {
    use model::*;
    use trust_core::services::{advisory::*, grading::*, leveling::*};
    use Operation::*;
    match op {
        RiskCheck => schema::<crate::safety::RiskCheckResult>(generator),
        Status => object(&[
            ("service", string()),
            ("version", string()),
            ("owner_model", string()),
            ("subject", string()),
            ("role", json!({"enum":["owner","agent"]})),
            ("live_enabled", boolean()),
            ("workflow", array(string())),
            ("background_reconcile_seconds", integer()),
        ]),
        Policy => object(&[
            ("money", string()),
            ("authority", string()),
            ("execution", string()),
            ("retries", string()),
            ("role", string()),
            ("owner_actions", string()),
            (
                "owner_authentication",
                object(&[
                    ("acr_values", array(string())),
                    ("max_age_seconds", integer()),
                    ("authenticated_until", nullable(integer())),
                ]),
            ),
            ("stop_changes", string()),
            ("scopes", array(string())),
            ("all_accounts", boolean()),
            ("accounts", array(uuid())),
            ("secrets", string()),
            ("currency", string()),
        ]),
        AccountsList => page(schema::<Account>(generator)),
        AccountGet => object(&[
            ("account", schema::<Account>(generator)),
            ("balances", schema::<Vec<AccountBalance>>(generator)),
        ]),
        AccountCreate | AccountDelete => schema::<Account>(generator),
        AccountReadiness | AccountRefresh | SafetyPolicySet | AccountHalt | AccountResume
        | MandateSet | EquityCashFlow | EquityReconcile | OperationResolve => readiness(generator),
        SafetyPolicyGet => object(&[
            ("account_id", uuid()),
            (
                "policy",
                schema::<Option<account_safety::AccountSafetyPolicy>>(generator),
            ),
        ]),
        MandateGet => object(&[
            ("subject", string()),
            (
                "mandate",
                schema::<Option<account_safety::AgentMandate>>(generator),
            ),
        ]),
        Balances => page(schema::<AccountBalance>(generator)),
        Transfer => schema::<(uuid::Uuid, uuid::Uuid)>(generator),
        Transactions => page(schema::<Transaction>(generator)),
        Deposit | Withdraw => schema::<(Transaction, AccountBalance)>(generator),
        Rules => page(schema::<Rule>(generator)),
        RuleCreate | RuleRemove => schema::<Rule>(generator),
        Instruments => page(schema::<TradingVehicle>(generator)),
        InstrumentCreate | InstrumentUpdate | InstrumentResolve => {
            schema::<TradingVehicle>(generator)
        }
        InstrumentStats => object(&[("total", integer()), ("bonds_missing_terms", integer())]),
        Trades => page(schema::<Trade>(generator)),
        TradeGet | TradeCreate | TradeCancel | StopModify | TargetModify | ManualFill
        | ManualStop | ManualTarget => schema::<Trade>(generator),
        TradeFund => object(&[
            ("trade", schema::<Trade>(generator)),
            ("transaction", schema::<Transaction>(generator)),
            ("account_balance", schema::<AccountBalance>(generator)),
            ("trade_balance", schema::<TradeBalance>(generator)),
        ]),
        TradeSubmit => object(&[
            ("trade", schema::<Trade>(generator)),
            ("next_action", string()),
        ]),
        TradeClose => object(&[
            ("trade_id", uuid()),
            ("exit_requested", boolean()),
            ("next_action", string()),
        ]),
        TradeSync => sync(generator),
        Reconcile => object(&[
            (
                "trades",
                array(union(vec![
                    sync(generator),
                    object(&[
                        ("trade_id", uuid()),
                        ("error", schema::<crate::error::ServiceError>(generator)),
                    ]),
                ])),
            ),
            ("complete", boolean()),
            ("next_after_trade_id", nullable(uuid())),
        ]),
        SizePreview => schema::<trust_core::LevelAdjustedQuantity>(generator),
        Hypothesis => schema::<trust_core::TradeHypothesis>(generator),
        BrokerPreview => schema::<broker::BrokerOrderPreview>(generator),
        Executions => page(schema::<Execution>(generator)),
        TradeAudit => object(&[
            ("events", schema::<Vec<trust_core::AuditEvent>>(generator)),
            (
                "verification",
                schema::<trust_core::AuditVerification>(generator),
            ),
        ]),
        Events => page(schema::<TradeEvent>(generator)),
        EventCreate => schema::<TradeEvent>(generator),
        Mistakes => page(schema::<Mistake>(generator)),
        MistakeCreate => schema::<Mistake>(generator),
        Reports => reports(generator),
        Metrics => union(vec![
            metrics(generator),
            schema::<trust_core::calculators_fixed_income::BondAnalytics>(generator),
            object(&[
                ("account_id", uuid()),
                ("metrics", metrics(generator)),
                ("other_account_id", uuid()),
                ("other_metrics", metrics(generator)),
            ]),
        ]),
        GradeGet => schema::<Option<TradeGrade>>(generator),
        GradeCompute | GradeRecord => schema::<DetailedTradeGrade>(generator),
        GradeSummary => page(schema::<TradeGrade>(generator)),
        LevelGet => schema::<Level>(generator),
        LevelHistory => page(schema::<LevelChange>(generator)),
        LevelRules | LevelRulesSet => schema::<LevelAdjustmentRules>(generator),
        LevelEvaluate => union(vec![
            schema::<(Level, LevelChange)>(generator),
            schema::<LevelEvaluationOutcome>(generator),
        ]),
        LevelProgress => schema::<LevelProgressReport>(generator),
        DistributionRules | DistributionConfigure => schema::<model::DistributionRules>(generator),
        DistributionExecute => schema::<DistributionResult>(generator),
        DistributionHistory => page(schema::<model::DistributionHistory>(generator)),
        AdvisorStatus => object(&[
            (
                "thresholds",
                schema::<trust_core::services::advisory::AdvisoryThresholds>(generator),
            ),
            ("portfolio", schema::<PortfolioAdvisoryStatus>(generator)),
        ]),
        AdvisorCheck => schema::<AdvisoryResult>(generator),
        AdvisorConfigure => schema::<()>(generator),
        AdvisorHistory => page(schema::<AdvisoryHistoryEntry>(generator)),
        Correlation => schema::<advisor::CorrelationAdvisory>(generator),
        Regime => schema::<advisor::RegimeSnapshot>(generator),
        Catalysts => schema::<advisor::CatalystScanResult>(generator),
        SessionList => page(schema::<SessionPlan>(generator)),
        SessionOpen | SessionClose => schema::<SessionPlan>(generator),
        MarketSnapshot | MarketSession => schema::<MarketSnapshotV2>(generator),
        MarketBars => page(schema::<MarketBar>(generator)),
        MarketQuote => schema::<model::MarketQuote>(generator),
        MarketTrade => schema::<MarketTradeTick>(generator),
        MarketStream => page(schema::<MarketDataStreamEvent>(generator)),
        OperationGet => object(&[
            ("operation_id", uuid()),
            ("tool", string()),
            ("account_id", nullable(uuid())),
            ("account_ids", array(uuid())),
            (
                "state",
                json!({"enum":["running","completed","failed","outcome_unknown"]}),
            ),
            ("result", nullable(json!({"$ref":"#/$defs/StoredReply"}))),
            (
                "resolution",
                nullable(object(&[("operation_id", uuid()), ("evidence", string())])),
            ),
        ]),
        AuditList => object(&[
            (
                "events",
                array(object(&[
                    ("sequence", integer()),
                    ("event", audit_event()),
                    ("previous_hash", string()),
                    ("event_hash", string()),
                    (
                        "safety_evidence",
                        schema::<Option<account_safety::AccountSafetyState>>(generator),
                    ),
                ])),
            ),
            ("next_cursor", integer()),
        ]),
        AuditVerify => object(&[("events", integer()), ("head_hash", string())]),
        BackupExport => object(&[
            ("backup_id", uuid()),
            ("scope", string()),
            ("control_journal", string()),
        ]),
        BackupImport => object(&[("inserted_rows", integer()), ("cleared_rows", integer())]),
        ConnectionStatus => page(object(&[
            ("account_id", uuid()),
            ("broker", schema::<BrokerKind>(generator)),
            ("environment", schema::<Environment>(generator)),
            ("broker_account_bound", boolean()),
            ("connectivity", string()),
            ("ready_for_new_risk", boolean()),
            ("blockers", array(string())),
            ("observed_at", nullable(timestamp())),
            ("next_action", string()),
        ])),
        SkillsList => page(schema::<crate::skills::SkillMetadata>(generator)),
        SkillRead => schema::<crate::skills::SkillContent>(generator),
    }
}

fn audit_event() -> Value {
    // Historical journal versions omit later metadata; every known field is typed.
    let mut value = object(&[
        ("version", integer()),
        ("source", string()),
        ("queue_wait_ms", integer()),
        ("duration_ms", integer()),
        ("error_code", nullable(string())),
        ("account_ids", array(uuid())),
        ("at", timestamp()),
        ("subject", string()),
        ("actor_role", nullable(string())),
        ("owner_authenticated_until", nullable(integer())),
        ("client_id", nullable(string())),
        ("request_id", uuid()),
        ("operation_id", nullable(uuid())),
        ("tool", string()),
        ("account_id", nullable(uuid())),
        ("outcome", string()),
        ("content_hash", nullable(string())),
    ]);
    if let Some(required) = value.get_mut("required") {
        *required = json!(["at", "subject", "request_id", "tool", "outcome"]);
    }
    value
}
fn envelope(data: Value, generator: &mut SchemaGenerator) -> Value {
    object(&[
        ("schema_version", json!({"const":1})),
        ("request_id", uuid()),
        ("operation_id", nullable(uuid())),
        ("data", nullable(data)),
        (
            "error",
            schema::<Option<crate::error::ServiceError>>(generator),
        ),
        ("next_actions", array(string())),
        (
            "actions",
            schema::<Vec<crate::guidance::NextAction>>(generator),
        ),
        ("generated_at", nullable(timestamp())),
    ])
}
fn build(spec: &ToolSpec) -> Map<String, Value> {
    let mut generator = SchemaSettings::draft2020_12()
        .with(|settings| settings.contract = Contract::Serialize)
        .into_generator();
    let result = data(spec.operation, &mut generator);
    let mut root = envelope(result, &mut generator);
    if spec.operation == Operation::OperationGet {
        // Only mutations have durable operation records. Deduplicate shared
        // contracts so recovery discovery does not repeat the entire catalogue.
        let alternatives = crate::catalog::tools()
            .iter()
            .filter(|t| t.mutation)
            .map(|tool| data(tool.operation, &mut generator))
            .fold(Vec::new(), |mut values, value| {
                if !values.contains(&value) {
                    values.push(value);
                }
                values
            });
        let mut stored = envelope(union(alternatives), &mut generator);
        if let Some(required) = stored.get_mut("required") {
            *required = json!([
                "schema_version",
                "request_id",
                "operation_id",
                "data",
                "error",
                "next_actions"
            ]);
        }
        generator
            .definitions_mut()
            .insert("StoredReply".into(), stored);
    }
    if let Some(object) = root.as_object_mut() {
        object.insert(
            "$schema".into(),
            json!("https://json-schema.org/draft/2020-12/schema"),
        );
        object.insert("title".into(), json!(format!("{} result", spec.name)));
        object.insert(
            "$defs".into(),
            Value::Object(generator.take_definitions(false)),
        );
    }
    portable_formats(&mut root);
    root.as_object().cloned().unwrap_or_default()
}

fn portable_formats(value: &mut Value) {
    match value {
        Value::Object(object) => {
            if let Some(format) = object.get("format").and_then(Value::as_str) {
                if !matches!(
                    format,
                    "date-time"
                        | "date"
                        | "time"
                        | "duration"
                        | "email"
                        | "hostname"
                        | "ipv4"
                        | "ipv6"
                        | "uuid"
                        | "uri"
                        | "uri-reference"
                        | "regex"
                        | "json-pointer"
                        | "relative-json-pointer"
                ) {
                    if format == "partial-date-time" {
                        object.insert(
                            "description".into(),
                            json!(
                                "UTC timestamp without zone suffix in the legacy financial ledger."
                            ),
                        );
                    }
                    object.remove("format");
                }
            }
            for child in object.values_mut() {
                portable_formats(child);
            }
        }
        Value::Array(values) => {
            for child in values {
                portable_formats(child);
            }
        }
        _ => {}
    }
}

pub fn output(spec: &ToolSpec) -> Arc<Map<String, Value>> {
    static SCHEMAS: OnceLock<BTreeMap<&'static str, Arc<Map<String, Value>>>> = OnceLock::new();
    let schemas = SCHEMAS.get_or_init(|| {
        crate::catalog::tools()
            .iter()
            .map(|tool| (tool.name, Arc::new(build(tool))))
            .collect()
    });
    schemas
        .get(spec.name)
        .cloned()
        .unwrap_or_else(|| Arc::new(build(spec)))
}
