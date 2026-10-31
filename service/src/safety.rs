//! Persistence and authorization boundary for core's broker-account risk engine.
use crate::{
    auth::{Identity, Role},
    catalog::Operation,
    domain::{self, Domain},
    error::{Result, ServiceError},
    journal::Journal,
};
use chrono::Utc;
use model::{account_safety::*, Account, Trade};
use serde_json::{json, Value};
use std::collections::BTreeSet;
use trust_core::account_safety as risk;
use trust_core::portfolio_risk as portfolio;
use uuid::Uuid;

fn error(code: &str) -> ServiceError {
    let next=match code {
        "portfolio_heat_exceeded"|"portfolio_stress_exceeded"|"trade_heat_exceeded"|"instrument_concentration_exceeded"|"sector_concentration_exceeded"|"cluster_concentration_exceeded"|"liquidity_participation_exceeded"|"drawdown_size_exceeded"=>"Use risk_check to assess a proposal within the authorized ceiling. Report the binding limit; never split orders or change policy to bypass it.",
        "market_evidence_unavailable"|"market_evidence_invalid_or_stale"|"volatility_history_incomplete"|"volatility_history_invalid"|"volatility_history_stale"=>"Obtain fresh, complete broker quotes and completed daily bars for the exact contract. Delayed data or missing subscriptions cannot authorize a trade.",
        "entry_window_blocked"|"instrument_risk_policy_expired"|"instrument_risk_policy_missing"|"portfolio_policy_missing"=>"Read account_safety_policy_get and hand off missing policy or calendar decisions to the human owner. Agents cannot invent or extend limits.",
        "agent_daily_loss_exceeded"|"agent_turnover_exceeded"|"agent_entry_count_exceeded"|"agent_reentry_cooldown"=>"Stop new agent entries. Inspect the mandate and readiness; wait for permitted capacity or provide an owner handoff. New keys, profiles and policy edits do not reset consumed budgets.",
        _=>"Read account_readiness. Refresh and reconcile broker evidence; the owner must resolve a latched halt. Never bypass limits or repeat an uncertain order.",
    };
    ServiceError::new(code, format!("Account safety check: {code}."), next)
}

fn risk_error(error_value: risk::SafetyError) -> ServiceError {
    error(&error_value.code)
}
fn storage_error(_: ServiceError) -> ServiceError {
    ServiceError::new(
        "audit_unavailable",
        "Cannot safely persist or verify account safety state.",
        "Stop new trading and inspect control storage.",
    )
}
fn account_scope(domain: &Domain, args: &Value) -> Result<(Account, BrokerAccountScope)> {
    let account = domain
        .facade
        .account_by_id(domain::id(args, "account_id")?)
        .map_err(|_| error("account_unavailable"))?;
    let scope = BrokerAccountScope::from_account(&account).map_err(error)?;
    Ok((account, scope))
}
fn linked_accounts(domain: &mut Domain, scope: &BrokerAccountScope) -> Result<BTreeSet<Uuid>> {
    Ok(domain
        .facade
        .search_all_accounts()
        .map_err(|_| error("account_unavailable"))?
        .iter()
        .filter(|a| BrokerAccountScope::from_account(a).ok().as_ref() == Some(scope))
        .map(|a| a.id)
        .collect())
}
fn load(
    domain: &mut Domain,
    journal: &Journal,
    scope: &BrokerAccountScope,
) -> Result<AccountSafetyState> {
    let mut state = journal
        .account_safety(scope, &linked_accounts(domain, scope)?)
        .map_err(storage_error)?;
    if !state.unresolved_operations.is_empty() {
        state.halt("unresolved_operations", Utc::now());
    }
    Ok(state)
}
fn save(
    journal: &mut Journal,
    identity: &Identity,
    request: Uuid,
    account: &Account,
    scope: &BrokerAccountScope,
    state: &AccountSafetyState,
) -> Result<()> {
    journal
        .save_safety(identity, request, account.id, scope, state)
        .map_err(storage_error)
}
fn refresh(
    domain: &mut Domain,
    state: &mut AccountSafetyState,
    account: &Account,
    scope: &BrokerAccountScope,
) -> Result<()> {
    if let (Some(previous), Some(policy)) = (&state.snapshot, &state.policy) {
        if Utc::now()
            .signed_duration_since(previous.observed_at)
            .num_seconds()
            > i64::try_from(policy.max_snapshot_age_seconds).unwrap_or(i64::MAX)
        {
            state.halt("broker_snapshot_stale", Utc::now());
        }
    }
    let result = match domain.facade.broker_account_snapshot(account) {
        Ok(snapshot) => {
            state.snapshot_fetch_failed = false;
            risk::observe(state, snapshot, scope, Utc::now()).map_err(risk_error)
        }
        Err(_) => {
            state.snapshot_fetch_failed = true;
            state.halt("broker_snapshot_unavailable", Utc::now());
            Err(error("broker_snapshot_unavailable"))
        }
    };
    result?;
    let all = match trades(domain, account) {
        Ok(trades) => trades,
        Err(error) => {
            state.halt("reconciliation_failed", Utc::now());
            return Err(error);
        }
    };
    let mut candidate = state.clone();
    candidate.halt = None;
    for blocker in risk::readiness_blockers(&candidate, scope, &all, Utc::now()) {
        state.halt(&blocker, Utc::now());
    }
    if portfolio::policy(state).is_ok() {
        let result: Result<()> = (|| {
            portfolio::observe_throttle(state, Utc::now()).map_err(risk_error)?;
            let pnl = domain
                .facade
                .agent_risk_pnl(state, &all)
                .map_err(|_| error("agent_pnl_unavailable"))?;
            portfolio::observe_agent_pnl(state, &pnl, Utc::now()).map_err(risk_error)?;
            portfolio::observe_turnover(state, &all, Utc::now()).map_err(risk_error)?;
            portfolio::reconcile_reservations(state, &all, Utc::now());
            portfolio::check_existing(state, &all, Utc::now()).map_err(risk_error)?;
            Ok(())
        })();
        if let Err(e) = &result {
            state.halt(&e.code, Utc::now());
        }
        result?;
    }
    Ok(())
}

pub fn background_account(
    domain: &mut Domain,
    journal: &Journal,
    after: Option<Uuid>,
) -> Result<Option<Uuid>> {
    let mut accounts = domain
        .facade
        .search_all_accounts()
        .map_err(|_| error("account_unavailable"))?;
    accounts.sort_by_key(|a| a.id);
    let mut configured = Vec::new();
    let mut seen = BTreeSet::new();
    for account in accounts {
        if account.environment == model::Environment::Live && !domain.config.allow_live {
            continue;
        }
        if let Ok(scope) = BrokerAccountScope::from_account(&account) {
            if seen.insert(scope.clone())
                && journal
                    .safety_state(&scope)
                    .map_err(storage_error)?
                    .policy
                    .is_some()
            {
                configured.push(account.id);
            }
        }
    }
    Ok(configured
        .iter()
        .find(|id| after.is_none_or(|previous| **id > previous))
        .or_else(|| configured.first())
        .copied())
}
fn trades(domain: &mut Domain, account: &Account) -> Result<Vec<Trade>> {
    domain
        .facade
        .broker_account_trades(account)
        .map_err(|_| error("account_trades_unavailable"))
}

pub fn intent_blockers(
    domain: &mut Domain,
    journal: &Journal,
    identity: &Identity,
    account_id: Uuid,
    trade: &Trade,
) -> Vec<String> {
    let result: Result<()> = (|| {
        let (account, scope) = account_scope(domain, &json!({"account_id":account_id}))?;
        domain
            .facade
            .require_trading_policy(account.id)
            .map_err(domain::domain_error)?;
        let state = load(domain, journal, &scope)?;
        risk::admit(
            &state,
            &scope,
            trade,
            &trades(domain, &account)?,
            if identity.grant.role == Role::Agent {
                Some(identity.subject.as_str())
            } else {
                None
            },
            Utc::now(),
        )
        .map_err(risk_error)?;
        let report = portfolio::check_existing(&state, &trades(domain, &account)?, Utc::now())
            .map_err(risk_error)?;
        if report.heat_remaining <= rust_decimal::Decimal::ZERO {
            return Err(error("portfolio_heat_exhausted"));
        }
        if report.sizing_multiplier <= rust_decimal::Decimal::ZERO {
            return Err(error("drawdown_blocks_new_risk"));
        }
        Ok(())
    })();
    result
        .err()
        .map(|error| vec![error.code])
        .unwrap_or_default()
}

fn view(
    domain: &mut Domain,
    identity: &Identity,
    account: &Account,
    scope: &BrokerAccountScope,
    state: &AccountSafetyState,
) -> Result<Value> {
    let all = trades(domain, account)?;
    let mut blockers = risk::readiness_blockers(state, scope, &all, Utc::now());
    let mut portfolio_report = match portfolio::check_existing(state, &all, Utc::now()) {
        Ok(report) => {
            if report.sizing_multiplier <= rust_decimal::Decimal::ZERO {
                blockers.insert("drawdown_blocks_new_risk".into());
            }
            if report.heat_remaining <= rust_decimal::Decimal::ZERO {
                blockers.insert("portfolio_heat_exhausted".into());
            }
            if report.stress_remaining <= rust_decimal::Decimal::ZERO {
                blockers.insert("portfolio_stress_exhausted".into());
            }
            Some(report)
        }
        Err(e) => {
            blockers.insert(e.code);
            None
        }
    };
    if identity.check("trust:trade", Some(account.id)).is_err() {
        blockers.insert("trade_permission_required".into());
    }
    if domain.facade.require_trading_policy(account.id).is_err() {
        blockers.insert("risk_policy_missing".into());
    }
    if domain
        .facade
        .search_rules(account.id)
        .map_err(domain::domain_error)?
        .iter()
        .any(|rule| {
            rule.name
                .risk_decimal()
                .is_ok_and(|amount| amount <= rust_decimal::Decimal::ZERO)
        })
    {
        blockers.insert("risk_policy_disables_trading".into());
    }
    if identity.grant.role == Role::Agent {
        match state.mandates.get(&identity.subject) {
            None => {
                blockers.insert("agent_mandate_missing".into());
            }
            Some(m) if m.expires_at <= Utc::now() => {
                blockers.insert("agent_mandate_expired".into());
            }
            Some(m) if m.risk_budget.is_none() => {
                blockers.insert("agent_risk_budget_missing".into());
            }
            _ => {}
        }
        if let Err(e) = portfolio::check_agent_capacity(state, &all, &identity.subject, Utc::now())
        {
            blockers.insert(e.code);
        }
    }
    let constraints = if blockers.is_empty() {
        match risk::remaining_capacity(
            state,
            &all,
            if identity.grant.role == Role::Agent {
                Some(identity.subject.as_str())
            } else {
                None
            },
        ) {
            Ok(capacity) => Some(capacity),
            Err(error) => {
                blockers.insert(error.code);
                None
            }
        }
    } else {
        None
    };
    let snapshot = state.snapshot.as_ref();
    // Agents see their admission status, not other agents' positions/balances/operations.
    let owner = identity.grant.role == Role::Owner
        && linked_accounts(domain, scope)?
            .iter()
            .all(|id| identity.permits_account(*id));
    if !owner {
        if let Some(report) = &mut portfolio_report {
            report.instrument_notional.clear();
            report.sector_notional.clear();
            report.cluster_notional.clear();
        }
    }
    Ok(json!({
        "account_id":account.id,"broker":account.broker_kind,"environment":account.environment,
        "ready_for_new_risk":blockers.is_empty(),"blockers":blockers,"halted":state.halt.is_some(),
        "halted_since":state.halt.as_ref().map(|h|h.since),"policy_configured":state.policy.is_some(),
        "observed_at":snapshot.map(|s|s.observed_at),"received_at":snapshot.map(|s|s.received_at),
        "snapshot_age_seconds":snapshot.map(|s|Utc::now().signed_duration_since(s.observed_at).num_seconds()),
        "snapshot_complete":snapshot.is_some_and(|s|s.complete&&s.consistent),
        "broker_connected":!state.snapshot_fetch_failed&&snapshot.zip(state.policy.as_ref()).is_some_and(|(s,p)|risk::validate_snapshot(s,scope,p,Utc::now()).is_ok()),
        "unresolved_operation_count":state.unresolved_operations.len(),
        "unresolved_operation_ids":if owner {Some(&state.unresolved_operations)} else {None},
        "snapshot":if owner {snapshot} else {None},
        "mandate":state.mandates.get(&identity.subject),
        "constraints":constraints,
        "portfolio_risk":portfolio_report,
        "equity_baseline":if owner {state.baseline.as_ref()} else {None},
        "risk_reducing_actions_remain_available":true
    }))
}

pub fn handles(op: Operation) -> bool {
    matches!(
        op,
        Operation::ConnectionStatus
            | Operation::RiskCheck
            | Operation::AccountReadiness
            | Operation::AccountRefresh
            | Operation::SafetyPolicyGet
            | Operation::SafetyPolicySet
            | Operation::AccountHalt
            | Operation::AccountResume
            | Operation::MandateGet
            | Operation::MandateSet
            | Operation::EquityCashFlow
            | Operation::EquityReconcile
            | Operation::OperationResolve
    )
}

pub fn dispatch(
    domain: &mut Domain,
    journal: &mut Journal,
    identity: &Identity,
    op: Operation,
    args: &Value,
    request: Uuid,
) -> Result<Value> {
    if op == Operation::ConnectionStatus {
        let mut views = Vec::new();
        for account in domain
            .facade
            .search_all_accounts()
            .map_err(domain::domain_error)?
            .into_iter()
            .filter(|account| identity.permits_account(account.id))
        {
            let (connectivity, ready, blockers, observed) =
                match BrokerAccountScope::from_account(&account) {
                    Ok(scope) => {
                        let state = load(domain, journal, &scope)?;
                        let view = view(domain, identity, &account, &scope, &state)?;
                        (
                            if view.get("broker_connected").and_then(Value::as_bool) == Some(true) {
                                "connected"
                            } else {
                                "unavailable"
                            },
                            view.get("ready_for_new_risk")
                                .cloned()
                                .unwrap_or(json!(false)),
                            view.get("blockers").cloned().unwrap_or(json!([])),
                            view.get("observed_at").cloned().unwrap_or(Value::Null),
                        )
                    }
                    Err(_) => (
                        "unbound",
                        json!(false),
                        json!(["broker_account_unbound"]),
                        Value::Null,
                    ),
                };
            views.push(json!({"account_id":account.id,"broker":account.broker_kind,"environment":account.environment,"broker_account_bound":account.broker_account_id.is_some(),"connectivity":connectivity,"ready_for_new_risk":ready,"blockers":blockers,"observed_at":observed,"next_action":"account_readiness"}));
        }
        return Ok(json!(views));
    }
    let (account, scope) = account_scope(domain, args)?;
    let mut state = load(domain, journal, &scope)?;
    match op {
        Operation::RiskCheck => {
            return risk_check(domain, identity, &account, &scope, &mut state, args)
        }
        Operation::AccountReadiness => return view(domain, identity, &account, &scope, &state),
        Operation::SafetyPolicyGet => {
            return Ok(json!({"account_id":account.id,"policy":state.policy}))
        }
        Operation::MandateGet => {
            let subject = args
                .get("subject")
                .and_then(Value::as_str)
                .unwrap_or(&identity.subject);
            if subject != identity.subject && identity.grant.role != Role::Owner {
                return Err(ServiceError::forbidden());
            }
            return Ok(json!({"subject":subject,"mandate":state.mandates.get(subject)}));
        }
        Operation::AccountRefresh => {
            let result = refresh(domain, &mut state, &account, &scope);
            save(journal, identity, request, &account, &scope, &state)?;
            result?;
            return view(domain, identity, &account, &scope, &state);
        }
        Operation::SafetyPolicySet => {
            let policy: AccountSafetyPolicy = decode(args, &["account_id", "idempotency_key"])?;
            risk::validate_policy(&policy).map_err(risk_error)?;
            portfolio::validate_policy(
                policy
                    .controls
                    .as_ref()
                    .ok_or_else(|| error("portfolio_policy_missing"))?,
            )
            .map_err(risk_error)?;
            if state
                .policy
                .as_ref()
                .is_some_and(|old| old.currency != policy.currency)
                && state.baseline.is_some()
            {
                return Err(error("risk_currency_mismatch"));
            }
            state.policy = Some(policy);
        }
        Operation::MandateSet => {
            let subject = domain::string(args, "subject")?;
            if !domain.config.grants.get(subject).is_some_and(|g| {
                g.role == Role::Agent && (g.all_accounts || g.accounts.contains(&account.id))
            }) {
                return Err(error("agent_subject_unavailable"));
            }
            let mandate: AgentMandate =
                decode(args, &["account_id", "idempotency_key", "subject"])?;
            risk::validate_mandate(&mandate, Utc::now()).map_err(risk_error)?;
            portfolio::validate_agent_budget(
                mandate
                    .risk_budget
                    .as_ref()
                    .ok_or_else(|| error("agent_risk_budget_missing"))?,
            )
            .map_err(risk_error)?;
            state.mandates.insert(subject.into(), mandate);
        }
        Operation::AccountHalt => {
            state.halt("operator_halt", Utc::now());
        }
        Operation::AccountResume => {
            let result = refresh(domain, &mut state, &account, &scope);
            save(journal, identity, request, &account, &scope, &state)?;
            result?;
            let mut candidate = state.clone();
            candidate.halt = None;
            let blockers = risk::readiness_blockers(
                &candidate,
                &scope,
                &trades(domain, &account)?,
                Utc::now(),
            );
            if !blockers.is_empty() {
                return Err(error("recovery_evidence_incomplete"));
            }
            portfolio::check_existing(&candidate, &trades(domain, &account)?, Utc::now())
                .map_err(risk_error)?;
            // A complete broker snapshot alone is insufficient when local fills disagree.
            verify_reconciled(&candidate, &trades(domain, &account)?)?;
            state.halt = None;
        }
        Operation::EquityCashFlow => {
            let amount = rust_decimal::Decimal::from_str_exact(domain::string(args, "amount")?)
                .map_err(|_| ServiceError::invalid("Invalid amount."))?;
            risk::record_cash_flow(&mut state, domain::string(args, "reference")?, amount)
                .map_err(risk_error)?;
        }
        Operation::EquityReconcile => {
            let refreshed = refresh(domain, &mut state, &account, &scope);
            save(journal, identity, request, &account, &scope, &state)?;
            if let Err(error) = refreshed {
                if error.code != "equity_cash_flow_unreconciled" {
                    return Err(error);
                }
            }
            let amount =
                rust_decimal::Decimal::from_str_exact(domain::string(args, "cash_flow_amount")?)
                    .map_err(|_| ServiceError::invalid("Invalid cash_flow_amount."))?;
            let result = risk::reconcile_equity(
                &mut state,
                &scope,
                domain::string(args, "reference")?,
                amount,
                Utc::now(),
            )
            .map_err(risk_error);
            save(journal, identity, request, &account, &scope, &state)?;
            result?;
        }
        Operation::OperationResolve => {
            let operation = domain::id(args, "operation_id")?;
            if !state.unresolved_operations.contains(&operation) {
                return Err(error("operation_not_unresolved"));
            }
            let result = refresh(domain, &mut state, &account, &scope);
            save(journal, identity, request, &account, &scope, &state)?;
            result?;
            verify_reconciled(&state, &trades(domain, &account)?)?;
            journal
                .resolve_operation(
                    identity,
                    request,
                    account.id,
                    operation,
                    domain::string(args, "evidence")?,
                )
                .map_err(storage_error)?;
            state.unresolved_operations.remove(&operation);
        }
        _ => return Err(ServiceError::internal()),
    }
    save(journal, identity, request, &account, &scope, &state)?;
    view(domain, identity, &account, &scope, &state)
}

fn decode<T: serde::de::DeserializeOwned>(args: &Value, remove: &[&str]) -> Result<T> {
    let mut value = args.clone();
    if let Some(object) = value.as_object_mut() {
        for key in remove {
            object.remove(*key);
        }
    }
    serde_json::from_value(value).map_err(|_|ServiceError::invalid("Invalid safety policy or mandate; use exact decimal strings, instrument UUIDs and RFC3339 expiry."))
}

#[derive(Debug, serde::Serialize, schemars::JsonSchema)]
pub struct RiskCheckResult {
    pub account_id: Uuid,
    pub instrument_id: Uuid,
    pub currency: String,
    pub observed_at: chrono::DateTime<Utc>,
    pub requested_quantity: rust_decimal::Decimal,
    /// Conservative maximum at or below the requested ceiling, rounded down to 8 decimals.
    pub max_admissible_quantity: rust_decimal::Decimal,
    pub requested_admissible: bool,
    pub blockers: Vec<String>,
    pub admission: Option<portfolio::AdmissionRisk>,
    pub reserves_capacity: bool,
}

fn risk_check(
    domain: &mut Domain,
    identity: &Identity,
    account: &Account,
    scope: &BrokerAccountScope,
    state: &mut AccountSafetyState,
    args: &Value,
) -> Result<Value> {
    use rust_decimal::{Decimal, RoundingStrategy};
    domain
        .facade
        .require_trading_policy(account.id)
        .map_err(domain::domain_error)?;
    refresh(domain, state, account, scope)?; // The preview changes only this in-memory copy.
    let id = domain::id(args, "instrument_id")?;
    let vehicle = domain
        .facade
        .search_trading_vehicles()
        .map_err(domain::domain_error)?
        .into_iter()
        .find(|v| v.id == id)
        .ok_or_else(|| error("instrument_unavailable"))?;
    let instrument = vehicle
        .broker_asset_id
        .as_deref()
        .ok_or_else(|| error("instrument_unresolved"))?
        .to_string();
    let lookback = portfolio::policy(state)
        .map_err(risk_error)?
        .volatility_lookback_bars;
    let market = domain
        .facade
        .broker_risk_market_data(account, &instrument, lookback)
        .map_err(|_| error("market_evidence_unavailable"))?;
    let observed_at = state
        .snapshot
        .as_ref()
        .map(|s| {
            s.observed_at.min(market.observed_at).min(
                s.margin
                    .as_ref()
                    .map(|m| m.observed_at)
                    .unwrap_or(s.observed_at),
            )
        })
        .ok_or_else(|| error("broker_snapshot_missing"))?;
    state.portfolio.market.insert(instrument, market);
    let requested = domain::decimal(args, "quantity")?;
    if requested <= Decimal::ZERO {
        return Err(ServiceError::invalid("Quantity must be positive."));
    }
    let mut trade = Trade {
        account_id: account.id,
        trading_vehicle: vehicle,
        currency: domain::currency(args)?,
        category: if domain::string(args, "side")? == "short" {
            model::TradeCategory::Short
        } else {
            model::TradeCategory::Long
        },
        ..Default::default()
    };
    trade.entry.unit_price = domain::decimal(args, "entry")?;
    trade.safety_stop.unit_price = domain::decimal(args, "stop")?;
    trade.target.unit_price = domain::decimal(args, "target")?;
    state
        .trade_owners
        .insert(trade.id, identity.subject.clone());
    let all = trades(domain, account)?;
    let local = domain
        .facade
        .calculate_level_adjusted_quantity(
            account.id,
            trade.entry.unit_price,
            trade.safety_stop.unit_price,
            &trade.currency,
        )
        .map_err(domain::domain_error)?;
    let mut high = requested
        .min(local.final_quantity)
        .round_dp_with_strategy(8, RoundingStrategy::ToZero);
    let mut low = Decimal::ZERO;
    let mut blockers = Vec::new();
    if high < requested {
        blockers.push("local_size_limit".into());
    }
    let mut admission = None;
    let now = Utc::now();
    let subject = if identity.grant.role == Role::Agent {
        Some(identity.subject.as_str())
    } else {
        None
    };
    let mut evaluate = |quantity: Decimal| -> Result<portfolio::AdmissionRisk> {
        trade.entry.quantity = quantity;
        trade.safety_stop.quantity = quantity;
        trade.target.quantity = quantity;
        domain
            .facade
            .validate_risk_proposal(&trade)
            .map_err(domain::domain_error)?;
        risk::admit(state, scope, &trade, &all, subject, now).map_err(risk_error)?;
        portfolio::admit(state, &trade, &all, subject, now).map_err(risk_error)
    };
    if high > Decimal::ZERO {
        match evaluate(high) {
            Ok(result) => {
                low = high;
                admission = Some(result);
            }
            Err(e) => {
                blockers.push(e.code.clone());
                let quantity_limited = matches!(
                    e.code.as_str(),
                    "portfolio_heat_exceeded"
                        | "portfolio_stress_exceeded"
                        | "margin_cushion_exhausted"
                        | "instrument_concentration_exceeded"
                        | "sector_concentration_exceeded"
                        | "cluster_concentration_exceeded"
                        | "liquidity_participation_exceeded"
                        | "trade_heat_exceeded"
                        | "drawdown_size_exceeded"
                        | "agent_turnover_exceeded"
                        | "account_order_limit_exceeded"
                        | "account_exposure_exceeded"
                        | "broker_capacity_exceeded"
                        | "agent_order_limit_exceeded"
                        | "agent_exposure_limit_exceeded"
                );
                if quantity_limited {
                    for _ in 0..128 {
                        let middle = high
                            .checked_sub(low)
                            .and_then(|d| d.checked_div(Decimal::from(2)))
                            .and_then(|d| low.checked_add(d))
                            .ok_or_else(|| error("risk_arithmetic_overflow"))?
                            .round_dp_with_strategy(8, RoundingStrategy::ToZero);
                        if middle <= low {
                            break;
                        }
                        match evaluate(middle) {
                            Ok(result) => {
                                low = middle;
                                admission = Some(result);
                            }
                            Err(_) => high = middle,
                        }
                    }
                }
            }
        }
    }
    if identity.grant.role != Role::Owner
        || !linked_accounts(domain, scope)?
            .iter()
            .all(|id| identity.check("trust:read", Some(*id)).is_ok())
    {
        if let Some(result) = &mut admission {
            result.portfolio.instrument_notional.clear();
            result.portfolio.sector_notional.clear();
            result.portfolio.cluster_notional.clear();
        }
    }
    serde_json::to_value(RiskCheckResult {
        account_id: account.id,
        instrument_id: id,
        currency: trade.currency.to_string(),
        observed_at,
        requested_quantity: requested,
        max_admissible_quantity: low,
        requested_admissible: low == requested,
        blockers,
        admission,
        reserves_capacity: false,
    })
    .map_err(|_| ServiceError::internal())
}

/// Fresh reads and core checks run immediately before committing additional capital.
pub fn before(
    domain: &mut Domain,
    journal: &mut Journal,
    identity: &Identity,
    op: Operation,
    args: &Value,
    request: Uuid,
) -> Result<()> {
    if !matches!(op, Operation::TradeFund | Operation::TradeSubmit) {
        return Ok(());
    }
    let (account, scope) = account_scope(domain, args)?;
    domain
        .facade
        .require_trading_policy(account.id)
        .map_err(domain::domain_error)?;
    let mut state = load(domain, journal, &scope)?;
    let result = refresh(domain, &mut state, &account, &scope);
    save(journal, identity, request, &account, &scope, &state)?;
    result?;
    let trade = domain
        .facade
        .read_trade(domain::id(args, "trade_id")?)
        .map_err(|_| error("trade_unavailable"))?;
    let all = trades(domain, &account)?;
    let controls = portfolio::policy(&state).map_err(risk_error)?;
    let instrument = trade
        .trading_vehicle
        .broker_asset_id
        .as_deref()
        .ok_or_else(|| error("instrument_unresolved"))?;
    let market = domain
        .facade
        .broker_risk_market_data(&account, instrument, controls.volatility_lookback_bars)
        .map_err(|_| error("market_evidence_unavailable"))?;
    state.portfolio.market.insert(instrument.into(), market);
    risk::admit(
        &state,
        &scope,
        &trade,
        &all,
        if identity.grant.role == Role::Agent {
            Some(identity.subject.as_str())
        } else {
            None
        },
        Utc::now(),
    )
    .map_err(risk_error)?;
    let admission = portfolio::admit(
        &state,
        &trade,
        &all,
        if identity.grant.role == Role::Agent {
            Some(identity.subject.as_str())
        } else {
            None
        },
        Utc::now(),
    )
    .map_err(risk_error)?;
    portfolio::reserve(
        &mut state,
        &trade,
        &identity.subject,
        request,
        &admission,
        Utc::now(),
    )
    .map_err(risk_error)?;
    save(journal, identity, request, &account, &scope, &state)
}

// Keep the post-dispatch audit context explicit alongside its operation result.
#[allow(clippy::too_many_arguments)]
pub fn after(
    domain: &mut Domain,
    journal: &mut Journal,
    identity: &Identity,
    spec: &crate::catalog::ToolSpec,
    args: &Value,
    request: Uuid,
    operation: Option<Uuid>,
    result: &Result<Value>,
) -> Result<()> {
    let op = spec.operation;
    // Only a durable mutation can have an uncertain side effect. An advisory or
    // market-data read failing does not create an unresolved trading intent.
    let uncertain = operation.is_some()
        && spec.affects_financial_state()
        && result
            .as_ref()
            .err()
            .is_some_and(|e| matches!(e.code.as_str(), "domain_rejected" | "outcome_unknown"));
    let failed_reconcile = matches!(op, Operation::TradeSync | Operation::Reconcile)
        && (result.is_err()
            || result
                .as_ref()
                .ok()
                .and_then(|v| v.get("trades"))
                .and_then(Value::as_array)
                .is_some_and(|items| items.iter().any(|v| v.get("error").is_some())));
    let admission = matches!(op, Operation::TradeFund | Operation::TradeSubmit);
    if !(op == Operation::TradeCreate && result.is_ok()
        || uncertain
        || failed_reconcile
        || admission)
    {
        return Ok(());
    }
    if args.get("account_id").is_none() {
        return Ok(());
    }
    let (account, scope) = account_scope(domain, args)?;
    let mut state = load(domain, journal, &scope)?;
    if admission && result.is_err() && !uncertain {
        portfolio::release_definitive_rejection(&mut state, request);
    }
    if op == Operation::TradeCreate {
        if let Ok(value) = result {
            let id = value
                .get("id")
                .and_then(Value::as_str)
                .and_then(|id| id.parse().ok())
                .ok_or_else(ServiceError::internal)?;
            state.trade_owners.insert(id, identity.subject.clone());
        }
    }
    if uncertain {
        state.halt("outcome_unknown", Utc::now());
        if let Some(id) = operation {
            state.unresolved_operations.insert(id);
        }
    }
    if failed_reconcile {
        state.halt("reconciliation_failed", Utc::now());
    }
    save(journal, identity, request, &account, &scope, &state)
}

fn verify_reconciled(state: &AccountSafetyState, trades: &[Trade]) -> Result<()> {
    let snapshot = state
        .snapshot
        .as_ref()
        .ok_or_else(|| error("broker_snapshot_missing"))?;
    risk::verify_reconciled(snapshot, trades).map_err(risk_error)
}
