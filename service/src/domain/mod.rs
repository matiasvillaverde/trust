//! Application operations: transport-independent authorization and facade calls.
mod accounts;
mod analysis;
mod journal;
mod trades;
use crate::{
    auth::Identity,
    catalog::{Operation, ToolSpec},
    config::Config,
    error::{Result, ServiceError},
};
use chrono::{DateTime, Utc};
use model::{Account, Environment, Status, Trade};
use rust_decimal::Decimal;
use serde::Serialize;
use serde_json::{json, Value};
use std::{collections::BTreeSet, sync::Arc};
use trust_core::TrustFacade;
use uuid::Uuid;

pub struct Domain {
    pub facade: TrustFacade,
    pub config: Arc<Config>,
}

impl Domain {
    /// Re-check current account ownership before dispatch OR replay of an operation.
    pub fn authorize(
        &mut self,
        identity: &Identity,
        spec: &ToolSpec,
        args: &Value,
    ) -> Result<BTreeSet<Uuid>> {
        let account = optional_id(args, "account_id")?;
        identity.check(spec.scope, account)?;
        if spec.scope == "trust:admin" && !spec.account_required && !identity.grant.all_accounts {
            return Err(ServiceError::forbidden());
        }
        let mut affected = BTreeSet::new();
        for key in [
            "account_id",
            "to_account_id",
            "other_account_id",
            "parent_account_id",
        ] {
            if let Some(id) = optional_id(args, key)? {
                identity.check(spec.scope, Some(id))?;
                let record = self.facade.account_by_id(id).map_err(|_| not_found())?;
                if spec.mutation {
                    self.check_live(identity, record.environment)?;
                }
                affected.insert(id);
            }
        }
        if let Some(id) = account {
            if matches!(
                spec.operation,
                Operation::TradeCreate | Operation::TradeFund | Operation::TradeSubmit
            ) {
                let account = self.facade.account_by_id(id).map_err(|_| not_found())?;
                model::account_safety::BrokerAccountScope::from_account(&account)
                    .map_err(ServiceError::invalid)?;
            }
            if matches!(
                spec.operation,
                Operation::SafetyPolicySet
                    | Operation::AccountHalt
                    | Operation::AccountResume
                    | Operation::MandateSet
                    | Operation::EquityCashFlow
                    | Operation::EquityReconcile
                    | Operation::OperationResolve
            ) {
                let account = self.facade.account_by_id(id).map_err(|_| not_found())?;
                let scope = model::account_safety::BrokerAccountScope::from_account(&account)
                    .map_err(ServiceError::invalid)?;
                for related in self.facade.search_all_accounts().map_err(domain_error)? {
                    if model::account_safety::BrokerAccountScope::from_account(&related)
                        .ok()
                        .as_ref()
                        == Some(&scope)
                    {
                        identity.check(spec.scope, Some(related.id))?;
                        self.check_live(identity, related.environment)?;
                        affected.insert(related.id);
                    }
                }
            }
            if spec.mutation {
                // Closures/reconciliation can automatically distribute profit to child accounts.
                for child in self
                    .facade
                    .search_all_accounts()
                    .map_err(domain_error)?
                    .iter()
                    .filter(|a| a.parent_account_id == Some(id))
                {
                    identity.check(spec.scope, Some(child.id))?;
                    self.check_live(identity, child.environment)?;
                    affected.insert(child.id);
                }
            }
            if let Some(trade_id) = optional_id(args, "trade_id")? {
                let trade = self.facade.read_trade(trade_id).map_err(|_| not_found())?;
                if trade.account_id != id {
                    return Err(ServiceError::forbidden());
                }
            }
        }
        if spec.operation == Operation::AccountCreate && string(args, "environment")? == "live" {
            self.check_live(identity, Environment::Live)?;
        }
        Ok(affected)
    }

    fn check_live(&self, identity: &Identity, environment: Environment) -> Result<()> {
        if environment == Environment::Live
            && !(self.config.allow_live && identity.grant.allow_live)
        {
            return Err(ServiceError::new("live_disabled","This identity cannot mutate a live account.","An operator must enable live operations in both service configuration and the subject grant."));
        }
        Ok(())
    }

    pub fn dispatch(&mut self, identity: &Identity, op: Operation, args: &Value) -> Result<Value> {
        use Operation::*;
        match op {
            Status => Ok(
                json!({"service":"trust","version":env!("CARGO_PKG_VERSION"),"owner_model":"single_owner","subject":identity.subject,"role":identity.grant.role,"live_enabled":self.config.allow_live&&identity.grant.allow_live,"workflow":["accounts_list","risk_preview","trade_hypothesis","trade_prepare","broker_preview","trade_fund","trade_submit","trade_reconcile"],"background_reconcile_seconds":self.config.reconcile_interval_seconds}),
            ),
            Policy => Ok(
                json!({"money":"Exact decimal strings; percentages use percentage points except distribution fractions, which sum to 1.","authority":"Trust validates sizing and risk. Broker previews do not override Trust policy.","execution":"A submitted or accepted order is not a confirmed fill.","retries":"Reuse the same idempotency key after timeout. Inspect operation_get; reconcile unknown outcomes before another intent.","role":identity.grant.role,"owner_actions":"Policy, capital, administration and manual accounting require the owner and recent human MFA. Agent scopes cannot grant owner authority.","owner_authentication":{"acr_values":self.config.owner_authentication.acr_values,"max_age_seconds":self.config.owner_authentication.max_age_seconds,"authenticated_until":identity.owner_authenticated_until},"stop_changes":"Long stops can only increase; short stops can only decrease. Positive prices are required. There is no owner override to widen an existing stop.","scopes":identity.scopes.iter().filter(|scope|identity.permits(scope)).collect::<Vec<_>>(),"all_accounts":identity.grant.all_accounts,"accounts":identity.grant.accounts,"secrets":"Operator mounted files only; never tool arguments.","currency":"Cross-currency aggregates are rejected; no implicit FX conversion."}),
            ),
            AccountsList | AccountGet | AccountCreate | AccountDelete | Balances | Transfer
            | Transactions | Deposit | Withdraw | Rules | RuleCreate | RuleRemove | Instruments
            | InstrumentCreate | InstrumentUpdate | InstrumentStats | InstrumentResolve
            | BackupExport | BackupImport => self.accounts(identity, op, args),
            Trades | TradeGet | TradeCreate | TradeFund | TradeSubmit | TradeCancel
            | TradeClose | TradeSync | Reconcile | StopModify | TargetModify | ManualFill
            | ManualStop | ManualTarget | SizePreview | Hypothesis | BrokerPreview | Executions
            | TradeAudit => self.trades(op, args),
            Events | EventCreate | Mistakes | MistakeCreate | GradeGet | GradeCompute
            | GradeRecord | GradeSummary | SessionList | SessionOpen | SessionClose => {
                self.journal(op, args)
            }
            Reports
            | Metrics
            | LevelGet
            | LevelHistory
            | LevelRules
            | LevelRulesSet
            | LevelEvaluate
            | LevelProgress
            | DistributionRules
            | DistributionConfigure
            | DistributionExecute
            | DistributionHistory
            | AdvisorStatus
            | AdvisorCheck
            | AdvisorConfigure
            | AdvisorHistory
            | Correlation
            | Regime
            | Catalysts
            | MarketSnapshot
            | MarketBars
            | MarketQuote
            | MarketTrade
            | MarketSession
            | MarketStream => self.analysis(op, args),
            SkillsList => Ok(crate::skills::list(identity)),
            SkillRead => value(crate::skills::read_file(
                identity,
                string(args, "name")?,
                args.get("path")
                    .and_then(Value::as_str)
                    .unwrap_or("SKILL.md"),
            )?),
            ConnectionStatus | OperationGet | AuditList | AuditVerify | AccountReadiness
            | RiskCheck | AccountRefresh | SafetyPolicyGet | SafetyPolicySet | AccountHalt
            | AccountResume | MandateGet | MandateSet | EquityCashFlow | EquityReconcile
            | OperationResolve => Err(ServiceError::internal()), // handled by the journal owner
        }
    }
    fn account(&self, args: &Value) -> Result<Account> {
        self.facade
            .account_by_id(id(args, "account_id")?)
            .map_err(|_| not_found())
    }
    fn trade(&mut self, args: &Value) -> Result<Trade> {
        let trade = self
            .facade
            .read_trade(id(args, "trade_id")?)
            .map_err(|_| not_found())?;
        if trade.account_id != id(args, "account_id")? {
            return Err(ServiceError::forbidden());
        }
        Ok(trade)
    }
}

pub fn string<'a>(v: &'a Value, key: &str) -> Result<&'a str> {
    v.get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| ServiceError::invalid(format!("{key} is required.")))
}
fn optional_string(v: &Value, key: &str) -> Option<String> {
    v.get(key).and_then(Value::as_str).map(str::to_owned)
}
pub fn id(v: &Value, key: &str) -> Result<Uuid> {
    string(v, key)?
        .parse()
        .map_err(|_| ServiceError::invalid(format!("Invalid {key}.")))
}
fn optional_id(v: &Value, key: &str) -> Result<Option<Uuid>> {
    v.get(key).map(|_| id(v, key)).transpose()
}
pub(crate) fn decimal(v: &Value, key: &str) -> Result<Decimal> {
    Decimal::from_str_exact(string(v, key)?)
        .map_err(|_| ServiceError::invalid(format!("Invalid decimal: {key}.")))
}
fn dec_or(v: &Value, key: &str, default: Decimal) -> Result<Decimal> {
    v.get(key)
        .map(|_| decimal(v, key))
        .transpose()
        .map(|value| value.unwrap_or(default))
}
fn uint(v: &Value, key: &str, default: u64) -> u64 {
    v.get(key).and_then(Value::as_u64).unwrap_or(default)
}
fn boolean(v: &Value, key: &str) -> bool {
    v.get(key).and_then(Value::as_bool).unwrap_or(false)
}
fn parse<T: std::str::FromStr>(v: &Value, key: &str) -> Result<T> {
    string(v, key)?
        .parse()
        .map_err(|_| ServiceError::invalid(format!("Invalid {key}.")))
}
pub(crate) fn currency(v: &Value) -> Result<model::Currency> {
    match v.get("currency").and_then(Value::as_str).unwrap_or("usd") {
        "usd" => Ok(model::Currency::USD),
        "eur" => Ok(model::Currency::EUR),
        "btc" => Ok(model::Currency::BTC),
        _ => Err(ServiceError::invalid("Invalid currency.")),
    }
}
fn strings(v: &Value, key: &str) -> Result<Vec<String>> {
    v.get(key)
        .and_then(Value::as_array)
        .ok_or_else(|| ServiceError::invalid(format!("{key} is required.")))?
        .iter()
        .map(|x| {
            x.as_str()
                .map(str::to_owned)
                .ok_or_else(|| ServiceError::invalid(format!("Invalid {key}.")))
        })
        .collect()
}
fn date(v: &Value, key: &str) -> Result<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(string(v, key)?)
        .map(|date| date.with_timezone(&Utc))
        .map_err(|_| {
            ServiceError::invalid(format!(
                "{key} must be an RFC3339 timestamp with a timezone."
            ))
        })
}
fn days(v: &Value) -> Result<u32> {
    u32::try_from(uint(v, "days", 30)).map_err(|_| ServiceError::invalid("Invalid days."))
}
fn period(v: &Value) -> Result<(DateTime<Utc>, DateTime<Utc>)> {
    let to = v
        .get("to")
        .map(|_| date(v, "to"))
        .transpose()?
        .unwrap_or_else(Utc::now);
    let from = v
        .get("from")
        .map(|_| date(v, "from"))
        .transpose()?
        .unwrap_or(
            to.checked_sub_signed(chrono::Duration::days(i64::from(days(v)?)))
                .ok_or_else(|| ServiceError::invalid("Invalid period."))?,
        );
    if from >= to {
        return Err(ServiceError::invalid("from must be earlier than to."));
    }
    Ok((from, to))
}
fn value<T: Serialize>(data: T) -> Result<Value> {
    serde_json::to_value(data).map_err(|_| ServiceError::internal())
}
fn output<T: Serialize, E: Into<Box<dyn std::error::Error>>>(
    result: std::result::Result<T, E>,
) -> Result<Value> {
    value(result.map_err(|error| domain_error(error.into()))?)
}
fn rejected() -> ServiceError {
    ServiceError::new("domain_rejected","Trust or the broker could not confirm this operation.","Inspect the account, trade state, risk rules, and broker connection. For a mutation, inspect operation_get and reconcile before taking another action.")
}
fn not_found() -> ServiceError {
    ServiceError::new(
        "not_found",
        "The requested record is unavailable.",
        "List the records visible to this identity and use their exact IDs.",
    )
}
pub fn active(status: Status) -> bool {
    matches!(
        status,
        Status::Submitted | Status::PartiallyFilled | Status::Filled
    )
}

fn history_page(args: &Value) -> Result<model::history::PageRequest> {
    model::history::PageRequest::new(uint(args, "limit", 100), uint(args, "offset", 0))
        .map_err(ServiceError::invalid)
}

pub fn paginate(value: Value, args: &Value) -> Value {
    match value {
        Value::Array(items) => {
            let total = items.len();
            let offset = usize::try_from(uint(args, "offset", 0)).unwrap_or(usize::MAX);
            let limit = usize::try_from(uint(args, "limit", 100)).unwrap_or(100);
            let selected: Vec<_> = items.into_iter().skip(offset).take(limit).collect();
            let end = offset.saturating_add(selected.len());
            json!({"items":selected,"total":total,"next_offset":if end<total {Some(end)}else{None}})
        }
        other => other,
    }
}

pub(crate) fn domain_error(error: Box<dyn std::error::Error>) -> ServiceError {
    if error
        .downcast_ref::<model::percentage::InvalidPercentage>()
        .is_some()
    {
        return ServiceError::invalid(error.to_string());
    }
    use trust_core::validation_errors::{
        FundValidationError, FundValidationErrorCode, RuleValidationError, TradeValidationError,
        TradeValidationErrorCode, TransactionValidationError,
    };
    if let Some(error) = error.downcast_ref::<FundValidationError>() {
        let code = match error.code {
            FundValidationErrorCode::RiskPolicyMissing => "risk_policy_missing",
            FundValidationErrorCode::RiskPerTradeExceeded => "risk_per_trade_exceeded",
            FundValidationErrorCode::RiskPerMonthExceeded => "risk_per_month_exceeded",
            FundValidationErrorCode::LevelAdjustedQuantityExceeded => "level_quantity_exceeded",
            FundValidationErrorCode::NotEnoughFunds => "insufficient_capital",
            FundValidationErrorCode::InvalidPriceDifference => "invalid_stop_distance",
            FundValidationErrorCode::InvalidQuantity => "invalid_quantity",
        };
        return ServiceError::new(code,&error.message,"Read risk_preview, balances_get, and risk_rules_list; prepare an intent within the permitted limits.");
    }
    if let Some(error) = error.downcast_ref::<TradeValidationError>() {
        if error.code == TradeValidationErrorCode::StopPriceNotValid {
            return ServiceError::new("stop_would_increase_risk", &error.message,
                "Read trade_get and trade_audit. Preserve the strongest recorded stop protection; a weaker broker observation or owner authority cannot reset it.");
        }
        let code = match error.code {
            TradeValidationErrorCode::InvalidStopPrice => "invalid_stop_price",
            _ => "invalid_trade_state",
        };
        return ServiceError::new(
            code,
            &error.message,
            "Read trade_get and follow the allowed lifecycle; reconcile unresolved broker actions.",
        );
    }
    if let Some(error) = error.downcast_ref::<TransactionValidationError>() {
        return ServiceError::new(
            "invalid_transaction",
            &error.message,
            "Read balances_get and trade_get; correct the ledger intent.",
        );
    }
    if let Some(error) = error.downcast_ref::<RuleValidationError>() {
        return ServiceError::new(
            "invalid_risk_rule",
            &error.message,
            "Read risk_rules_list before changing the policy.",
        );
    }
    if let Some(model::BrokerError::UnsupportedAssetClass {
        broker, category, ..
    }) = error.downcast_ref::<model::BrokerError>()
    {
        return ServiceError::new(
            "unsupported_asset",
            format!("{broker} does not support {category} for this operation."),
            "Use a supported instrument and account; do not bypass validation.",
        );
    }
    rejected()
}
