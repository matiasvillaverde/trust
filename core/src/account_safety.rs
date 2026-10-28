//! Whole-account limits supplement, and never replace, trade-level admission.
#![allow(missing_docs)]
use crate::TrustFacade;
use chrono::{DateTime, Utc};
use model::{account_safety::*, Account, Status, Trade};
use rust_decimal::Decimal;
use serde::Serialize;
use std::{collections::BTreeSet, error::Error, fmt};
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, schemars::JsonSchema)]
pub struct SafetyError {
    pub code: String,
}
impl fmt::Display for SafetyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.code)
    }
}
impl Error for SafetyError {}
type Result<T> = std::result::Result<T, SafetyError>;
fn failure(code: &str) -> SafetyError {
    SafetyError { code: code.into() }
}
fn add(a: Decimal, b: Decimal) -> Result<Decimal> {
    a.checked_add(b)
        .ok_or_else(|| failure("risk_arithmetic_overflow"))
}
fn sub(a: Decimal, b: Decimal) -> Result<Decimal> {
    a.checked_sub(b)
        .ok_or_else(|| failure("risk_arithmetic_overflow"))
}
fn mul(a: Decimal, b: Decimal) -> Result<Decimal> {
    a.checked_mul(b)
        .ok_or_else(|| failure("risk_arithmetic_overflow"))
}

impl TrustFacade {
    pub fn broker_account_snapshot(
        &self,
        account: &Account,
    ) -> std::result::Result<BrokerAccountSnapshot, Box<dyn Error>> {
        self.broker.account_snapshot(account)
    }
    /// Includes every Trust profile bound to the same external account.
    pub fn broker_account_trades(
        &mut self,
        account: &Account,
    ) -> std::result::Result<Vec<Trade>, Box<dyn Error>> {
        let scope = BrokerAccountScope::from_account(account)?;
        let mut trades = Vec::new();
        for other in self.search_all_accounts()? {
            if BrokerAccountScope::from_account(&other).ok().as_ref() == Some(&scope) {
                for mut trade in self.account_trades(other.id)? {
                    if matches!(trade.status, Status::Filled | Status::PartiallyFilled) {
                        // This is an admission-only copy. Keep persisted broker state
                        // unchanged while retaining the strongest audited protection.
                        trade.safety_stop.unit_price = crate::commands::trade::recorded_stop_bound(
                            &trade,
                            self.factory.as_mut(),
                        )?;
                    }
                    trades.push(trade);
                }
            }
        }
        Ok(trades)
    }
}

pub fn validate_policy(policy: &AccountSafetyPolicy) -> Result<()> {
    if !matches!(policy.currency.as_str(), "USD" | "EUR" | "BTC")
        || policy.max_gross_notional <= Decimal::ZERO
        || policy.max_order_notional <= Decimal::ZERO
        || policy.max_order_notional > policy.max_gross_notional
        || policy.max_daily_loss <= Decimal::ZERO
        || policy.max_drawdown <= Decimal::ZERO
        || !(1..=120).contains(&policy.max_snapshot_age_seconds)
    {
        return Err(failure("invalid_safety_policy"));
    }
    Ok(())
}

pub fn validate_mandate(mandate: &AgentMandate, now: DateTime<Utc>) -> Result<()> {
    if mandate.instrument_ids.is_empty()
        || mandate.instrument_ids.len() > 1000
        || mandate.max_order_notional <= Decimal::ZERO
        || mandate.max_total_notional < mandate.max_order_notional
        || mandate.max_open_trades == 0
        || mandate.max_open_trades > 1000
        || mandate.expires_at <= now
    {
        return Err(failure("invalid_agent_mandate"));
    }
    Ok(())
}

pub fn validate_snapshot(
    snapshot: &BrokerAccountSnapshot,
    scope: &BrokerAccountScope,
    policy: &AccountSafetyPolicy,
    now: DateTime<Utc>,
) -> Result<()> {
    if snapshot.scope != *scope {
        return Err(failure("broker_identity_mismatch"));
    }
    if !snapshot.complete || !snapshot.consistent {
        return Err(failure("broker_snapshot_incomplete"));
    }
    let age = now
        .signed_duration_since(snapshot.observed_at)
        .num_seconds();
    let received_age = now
        .signed_duration_since(snapshot.received_at)
        .num_seconds();
    let maximum = i64::try_from(policy.max_snapshot_age_seconds)
        .map_err(|_| failure("invalid_safety_policy"))?;
    if snapshot.observed_at > now
        || snapshot.received_at > now
        || age > maximum
        || received_age > maximum
        || snapshot.observed_at > snapshot.received_at
    {
        return Err(failure("broker_snapshot_stale"));
    }
    if snapshot.currency != policy.currency {
        return Err(failure("risk_currency_mismatch"));
    }
    if snapshot.equity <= Decimal::ZERO || snapshot.available_funds < Decimal::ZERO {
        return Err(failure("broker_capacity_unavailable"));
    }
    let mut positions = BTreeSet::new();
    for p in &snapshot.positions {
        if !matches!(p.asset_class.as_str(), "STK" | "ETF" | "CRYPTO") {
            return Err(failure("unsupported_broker_risk_instrument"));
        }
        if p.currency != policy.currency {
            return Err(failure("risk_currency_mismatch"));
        }
        if p.instrument_id.is_empty()
            || !positions.insert(&p.instrument_id)
            || p.quantity.is_zero() != p.market_value.is_zero()
            || (!p.quantity.is_zero()
                && p.quantity.is_sign_negative() != p.market_value.is_sign_negative())
        {
            return Err(failure("broker_positions_invalid"));
        }
    }
    let mut orders = BTreeSet::new();
    for o in &snapshot.working_orders {
        if !matches!(o.asset_class.as_str(), "STK" | "ETF" | "CRYPTO") {
            return Err(failure("unsupported_broker_risk_instrument"));
        }
        if o.currency != policy.currency {
            return Err(failure("risk_currency_mismatch"));
        }
        if o.order_id.is_empty()
            || o.instrument_id.is_empty()
            || !orders.insert(&o.order_id)
            || o.remaining_quantity < Decimal::ZERO
            || o.notional.is_some_and(|n| n < Decimal::ZERO)
        {
            return Err(failure("broker_orders_invalid"));
        }
        if !matches!(o.side.as_str(), "buy" | "sell") {
            return Err(failure("broker_orders_invalid"));
        }
    }
    Ok(())
}

/// Refresh never clears a halt. Equity baselines survive process restarts and policy edits.
/// An unexplained equity/P&L difference latches a halt until the owner records the
/// evidenced external cash flow. This prevents deposits masking trading losses.
pub fn observe(
    state: &mut AccountSafetyState,
    snapshot: BrokerAccountSnapshot,
    scope: &BrokerAccountScope,
    now: DateTime<Utc>,
) -> Result<()> {
    let result = observe_inner(state, &snapshot, scope, now);
    if let Err(error) = &result {
        state.halt(&error.code, now);
    }
    // Retain even a rejected observation for diagnostics; admission validates it again.
    state.snapshot = Some(snapshot);
    result
}

fn observe_inner(
    state: &mut AccountSafetyState,
    snapshot: &BrokerAccountSnapshot,
    scope: &BrokerAccountScope,
    now: DateTime<Utc>,
) -> Result<()> {
    let policy = state
        .policy
        .as_ref()
        .ok_or_else(|| failure("safety_policy_missing"))?;
    validate_policy(policy)?;
    validate_snapshot(snapshot, scope, policy, now)?;
    let pnl = add(snapshot.realized_pnl, snapshot.unrealized_pnl)?;
    if let Some(previous) = state.snapshot.as_ref() {
        if validate_snapshot(previous, scope, policy, now).is_ok()
            && snapshot.observed_at < previous.observed_at
        {
            return Err(failure("broker_snapshot_regressed"));
        }
    }
    let mut baseline = state.baseline.clone().unwrap_or(EquityBaseline {
        day: now.date_naive(),
        day_start: snapshot.equity,
        high_water: snapshot.equity,
        last_equity: snapshot.equity,
        last_pnl: pnl,
        cash_flow_adjustment: Decimal::ZERO,
    });
    let equity_delta = sub(snapshot.equity, baseline.last_equity)?;
    let pnl_delta = sub(pnl, baseline.last_pnl)?;
    let unexplained = sub(sub(equity_delta, pnl_delta)?, baseline.cash_flow_adjustment)?;
    if unexplained.abs() > Decimal::new(1, 2) {
        return Err(failure("equity_cash_flow_unreconciled"));
    }
    if baseline.day != now.date_naive() {
        // Carry the last observed equity through the boundary; never reset to a
        // depressed first observation and forgive an overnight loss.
        baseline.day_start = add(baseline.last_equity, baseline.cash_flow_adjustment)?;
        baseline.day = now.date_naive();
    }
    baseline.high_water = baseline.high_water.max(snapshot.equity);
    baseline.last_equity = snapshot.equity;
    baseline.last_pnl = pnl;
    baseline.cash_flow_adjustment = Decimal::ZERO;
    state.baseline = Some(baseline);
    check_equity(state, snapshot)
}

pub fn check_equity(state: &AccountSafetyState, snapshot: &BrokerAccountSnapshot) -> Result<()> {
    let policy = state
        .policy
        .as_ref()
        .ok_or_else(|| failure("safety_policy_missing"))?;
    let baseline = state
        .baseline
        .as_ref()
        .ok_or_else(|| failure("equity_baseline_missing"))?;
    if sub(baseline.day_start, snapshot.equity)? >= policy.max_daily_loss {
        return Err(failure("daily_equity_loss_exceeded"));
    }
    if sub(baseline.high_water, snapshot.equity)? >= policy.max_drawdown {
        return Err(failure("equity_drawdown_exceeded"));
    }
    Ok(())
}

/// Explicit owner evidence adjusts capital, not the measured trading performance.
pub fn record_cash_flow(
    state: &mut AccountSafetyState,
    reference: &str,
    amount: Decimal,
) -> Result<()> {
    if reference.trim().is_empty()
        || reference.len() > 200
        || amount.is_zero()
        || state.cash_flow_references.contains(reference)
    {
        return Err(failure("invalid_cash_flow_evidence"));
    }
    let mut baseline = state
        .baseline
        .clone()
        .ok_or_else(|| failure("equity_baseline_missing"))?;
    baseline.day_start = add(baseline.day_start, amount)?;
    baseline.high_water = add(baseline.high_water, amount)?;
    baseline.cash_flow_adjustment = add(baseline.cash_flow_adjustment, amount)?;
    if baseline.day_start <= Decimal::ZERO || baseline.high_water <= Decimal::ZERO {
        return Err(failure("invalid_cash_flow_evidence"));
    }
    state.baseline = Some(baseline);
    state.cash_flow_references.insert(reference.into());
    Ok(())
}

/// Reconcile a broker P&L counter reset using owner-verified statement evidence.
/// Capital and performance baselines remain intact except for the stated cash flow.
/// This never clears a halt; current equity losses are checked again immediately.
pub fn reconcile_equity(
    state: &mut AccountSafetyState,
    scope: &BrokerAccountScope,
    reference: &str,
    cash_flow: Decimal,
    now: DateTime<Utc>,
) -> Result<()> {
    let snapshot = state
        .snapshot
        .clone()
        .ok_or_else(|| failure("broker_snapshot_missing"))?;
    let policy = state
        .policy
        .as_ref()
        .ok_or_else(|| failure("safety_policy_missing"))?;
    validate_snapshot(&snapshot, scope, policy, now)?;
    if reference.trim().is_empty()
        || reference.len() > 200
        || state.cash_flow_references.contains(reference)
    {
        return Err(failure("invalid_cash_flow_evidence"));
    }
    let mut candidate = state.clone();
    if cash_flow.is_zero() {
        candidate.cash_flow_references.insert(reference.into());
    } else {
        record_cash_flow(&mut candidate, reference, cash_flow)?;
    }
    let baseline = candidate
        .baseline
        .as_mut()
        .ok_or_else(|| failure("equity_baseline_missing"))?;
    let trading_change = sub(
        sub(snapshot.equity, baseline.last_equity)?,
        baseline.cash_flow_adjustment,
    )?;
    baseline.last_pnl = sub(
        add(snapshot.realized_pnl, snapshot.unrealized_pnl)?,
        trading_change,
    )?;
    let result = observe(&mut candidate, snapshot, scope, now);
    *state = candidate;
    result
}

pub fn is_exposed(trade: &Trade) -> bool {
    matches!(
        trade.status,
        Status::Funded | Status::Submitted | Status::PartiallyFilled | Status::Filled
    )
}
pub fn trade_notional(trade: &Trade) -> Result<Decimal> {
    if trade.entry.quantity <= Decimal::ZERO || trade.entry.unit_price <= Decimal::ZERO {
        return Err(failure("invalid_trade_notional"));
    }
    mul(
        trade.entry.quantity,
        trade.entry.unit_price.max(trade.safety_stop.unit_price),
    )
}

/// Gross commitment counts external positions, working entries and reservations.
/// Known bracket exits are excluded only when their side and remaining quantity
/// reduce a confirmed position. No margin, FX or unknown multiplier is assumed.
pub fn gross_commitment(
    snapshot: &BrokerAccountSnapshot,
    trades: &[Trade],
    excluding: Option<Uuid>,
) -> Result<Decimal> {
    let mut gross = Decimal::ZERO;
    for position in &snapshot.positions {
        gross = add(gross, position.market_value.abs())?;
    }
    for order in &snapshot.working_orders {
        if order.remaining_quantity > Decimal::ZERO && !verified_exit(order, snapshot, trades) {
            let notional = order
                .notional
                .filter(|n| *n > Decimal::ZERO)
                .ok_or_else(|| failure("unbounded_working_order"))?;
            gross = add(gross, notional)?;
        }
    }
    for trade in trades
        .iter()
        .filter(|t| t.status == Status::Funded && Some(t.id) != excluding)
    {
        if trade.currency.to_string().to_uppercase() != snapshot.currency {
            return Err(failure("risk_currency_mismatch"));
        }
        gross = add(gross, trade_notional(trade)?)?;
    }
    Ok(gross)
}

pub(crate) fn verified_exit(
    order: &BrokerWorkingOrder,
    snapshot: &BrokerAccountSnapshot,
    trades: &[Trade],
) -> bool {
    if verified_manual_stop(order, snapshot, trades) {
        return true;
    }
    let Some(position) = snapshot
        .positions
        .iter()
        .find(|p| p.instrument_id == order.instrument_id)
    else {
        return false;
    };
    if order.remaining_quantity > position.quantity.abs()
        || !((position.quantity > Decimal::ZERO && order.side == "sell")
            || (position.quantity < Decimal::ZERO && order.side == "buy"))
    {
        return false;
    }
    trades
        .iter()
        .filter(|t| matches!(t.status, Status::Filled | Status::PartiallyFilled))
        .any(|trade| {
            trade.trading_vehicle.broker_asset_id.as_deref() == Some(order.instrument_id.as_str())
                && trade
                    .entry
                    .filled_quantity
                    .checked_sub(trade.safety_stop.filled_quantity)
                    .and_then(|remaining| remaining.checked_sub(trade.target.filled_quantity))
                    .is_some_and(|remaining| order.remaining_quantity <= remaining)
                && [&trade.safety_stop, &trade.target].iter().any(|leg| {
                    leg.broker_order_id.as_deref() == Some(order.order_id.as_str())
                        || order.client_order_id.as_deref().is_some_and(|reference| {
                            leg.broker_order_id.as_deref() == Some(reference)
                                || leg.id.to_string() == reference
                        })
                })
        })
}

/// Compare Trust's known executions and outstanding entries against a complete
/// observation. Extra manual positions and bounded orders remain in gross risk.
/// Offset positions or missing Trust orders require reconciliation, never guessing.
pub fn verify_reconciled(snapshot: &BrokerAccountSnapshot, trades: &[Trade]) -> Result<()> {
    for order in &snapshot.working_orders {
        let matches = trades
            .iter()
            .filter(|t| {
                matches!(
                    t.status,
                    Status::Submitted | Status::PartiallyFilled | Status::Filled
                )
            })
            .flat_map(|t| [&t.entry, &t.safety_stop, &t.target])
            .filter(|leg| order_matches(order, leg))
            .take(2)
            .count();
        if matches > 1 {
            return Err(failure("broker_order_identity_ambiguous"));
        }
    }
    let mut expected = std::collections::BTreeMap::<String, Decimal>::new();
    for trade in trades.iter().filter(|t| {
        matches!(
            t.status,
            Status::Submitted | Status::PartiallyFilled | Status::Filled
        )
    }) {
        let instrument = trade
            .trading_vehicle
            .broker_asset_id
            .clone()
            .ok_or_else(|| failure("instrument_unresolved"))?;
        let exited = add(
            trade.safety_stop.filled_quantity,
            trade.target.filled_quantity,
        )?;
        let quantity = sub(trade.entry.filled_quantity, exited)?;
        if quantity < Decimal::ZERO {
            return Err(failure("broker_ledger_disagreement"));
        }
        let signed = if trade.category == model::TradeCategory::Short {
            sub(Decimal::ZERO, quantity)?
        } else {
            quantity
        };
        let prior = expected.get(&instrument).copied().unwrap_or(Decimal::ZERO);
        expected.insert(instrument, add(prior, signed)?);
        verify_trade_orders(snapshot, trade, quantity)?;
    }
    for (instrument, quantity) in expected {
        let observed = snapshot
            .positions
            .iter()
            .find(|p| p.instrument_id == instrument)
            .map(|p| p.quantity)
            .unwrap_or(Decimal::ZERO);
        if quantity > Decimal::ZERO && observed < quantity
            || quantity < Decimal::ZERO && observed > quantity
        {
            return Err(failure("broker_ledger_disagreement"));
        }
    }
    for order in &snapshot.working_orders {
        if trades.iter().any(|trade| {
            matches!(trade.status, Status::New | Status::Funded)
                && [&trade.entry, &trade.safety_stop, &trade.target]
                    .iter()
                    .any(|leg| order_matches(order, leg))
        }) {
            return Err(failure("broker_entry_unreconciled"));
        }
    }
    Ok(())
}

fn verify_trade_orders(
    snapshot: &BrokerAccountSnapshot,
    trade: &Trade,
    quantity: Decimal,
) -> Result<()> {
    if quantity > Decimal::ZERO
        && !snapshot.working_orders.iter().any(|order| {
            order_matches(order, &trade.safety_stop)
                && order.active
                && matches!(order.order_type.as_str(), "stop" | "stop_limit")
                && order.stop_price.is_some_and(|price| {
                    price > Decimal::ZERO
                        && if trade.category == model::TradeCategory::Short {
                            price <= trade.safety_stop.unit_price
                        } else {
                            price >= trade.safety_stop.unit_price
                        }
                })
                && order.remaining_quantity >= quantity
                && order.instrument_id
                    == trade
                        .trading_vehicle
                        .broker_asset_id
                        .as_deref()
                        .unwrap_or("")
                && order.side
                    == if trade.category == model::TradeCategory::Short {
                        "buy"
                    } else {
                        "sell"
                    }
        })
    {
        return Err(failure("broker_protection_unreconciled"));
    }
    if trade.entry.filled_quantity < trade.entry.quantity
        && !snapshot.working_orders.iter().any(|order| {
            order.instrument_id
                == trade
                    .trading_vehicle
                    .broker_asset_id
                    .as_deref()
                    .unwrap_or("")
                && order_matches(order, &trade.entry)
        })
    {
        return Err(failure("broker_entry_unreconciled"));
    }
    Ok(())
}

pub(crate) fn order_matches(order: &BrokerWorkingOrder, leg: &model::Order) -> bool {
    leg.broker_order_id.as_deref() == Some(order.order_id.as_str())
        || order.client_order_id.as_deref().is_some_and(|id| {
            leg.broker_order_id.as_deref() == Some(id) || leg.id.to_string() == id
        })
}

/// Only one active full-position market stop may protect an entirely external position.
/// Multiple exits without broker OCA evidence remain potential new exposure.
pub(crate) fn verified_manual_stop(
    order: &BrokerWorkingOrder,
    snapshot: &BrokerAccountSnapshot,
    trades: &[Trade],
) -> bool {
    let Some(position) = snapshot
        .positions
        .iter()
        .find(|p| p.instrument_id == order.instrument_id)
    else {
        return false;
    };
    if trades.iter().any(|t| {
        is_exposed(t)
            && t.trading_vehicle.broker_asset_id.as_deref() == Some(order.instrument_id.as_str())
    }) {
        return false;
    }
    let eligible = |o: &BrokerWorkingOrder| {
        o.instrument_id == position.instrument_id
            && o.active
            && o.order_type == "stop"
            && o.stop_price.is_some_and(|p| p > Decimal::ZERO)
            && o.remaining_quantity == position.quantity.abs()
            && ((position.quantity > Decimal::ZERO && o.side == "sell")
                || (position.quantity < Decimal::ZERO && o.side == "buy"))
    };
    eligible(order)
        && snapshot
            .working_orders
            .iter()
            .filter(|o| eligible(o))
            .count()
            == 1
}

pub fn readiness_blockers(
    state: &AccountSafetyState,
    scope: &BrokerAccountScope,
    trades: &[Trade],
    now: DateTime<Utc>,
) -> BTreeSet<String> {
    let mut blockers = BTreeSet::new();
    if let Some(halt) = &state.halt {
        blockers.extend(halt.reasons.clone());
        blockers.insert("account_halted".into());
    }
    if !state.unresolved_operations.is_empty() {
        blockers.insert("unresolved_operations".into());
    }
    let check = (|| {
        let policy = state
            .policy
            .as_ref()
            .ok_or_else(|| failure("safety_policy_missing"))?;
        validate_policy(policy)?;
        let snapshot = state
            .snapshot
            .as_ref()
            .ok_or_else(|| failure("broker_snapshot_missing"))?;
        validate_snapshot(snapshot, scope, policy, now)?;
        check_equity(state, snapshot)?;
        if gross_commitment(snapshot, trades, None)? > policy.max_gross_notional {
            return Err(failure("account_exposure_exceeded"));
        }
        verify_reconciled(snapshot, trades)?;
        Ok(())
    })();
    if let Err(error) = check {
        blockers.insert(error.code);
    }
    blockers
}

pub fn check_mandate(
    state: &AccountSafetyState,
    subject: &str,
    trade: &Trade,
    trades: &[Trade],
    now: DateTime<Utc>,
) -> Result<()> {
    let mandate = state
        .mandates
        .get(subject)
        .ok_or_else(|| failure("agent_mandate_missing"))?;
    if mandate.expires_at <= now {
        return Err(failure("agent_mandate_expired"));
    }
    if !mandate.instrument_ids.contains(&trade.trading_vehicle.id) {
        return Err(failure("instrument_not_permitted"));
    }
    if !mandate.allow_short && trade.category == model::TradeCategory::Short {
        return Err(failure("short_not_permitted"));
    }
    let proposed = trade_notional(trade)?;
    if proposed > mandate.max_order_notional {
        return Err(failure("agent_order_limit_exceeded"));
    }
    let mut total = proposed;
    let mut count: u32 = 1;
    for other in trades.iter().filter(|t| t.id != trade.id && is_exposed(t)) {
        if state
            .trade_owners
            .get(&other.id)
            .is_some_and(|owner| owner == subject)
        {
            total = add(total, marked_commitment(state, other)?)?;
            count = count
                .checked_add(1)
                .ok_or_else(|| failure("risk_arithmetic_overflow"))?;
        }
    }
    if total > mandate.max_total_notional || count > mandate.max_open_trades {
        return Err(failure("agent_exposure_limit_exceeded"));
    }
    Ok(())
}

fn marked_commitment(state: &AccountSafetyState, trade: &Trade) -> Result<Decimal> {
    let mut commitment = trade_notional(trade)?;
    if let Some(position) = state.snapshot.as_ref().and_then(|snapshot| {
        snapshot.positions.iter().find(|position| {
            Some(position.instrument_id.as_str())
                == trade.trading_vehicle.broker_asset_id.as_deref()
                && !position.quantity.is_zero()
        })
    }) {
        let mark = position
            .market_value
            .abs()
            .checked_div(position.quantity.abs())
            .ok_or_else(|| failure("risk_arithmetic_overflow"))?;
        commitment = commitment.max(mul(trade.entry.quantity, mark)?);
    }
    Ok(commitment)
}

#[derive(Debug, Clone, Serialize, schemars::JsonSchema)]
pub struct AdmissionCapacity {
    pub currency: String,
    pub max_order_notional: Decimal,
    pub account_notional_remaining: Decimal,
    pub agent_notional_remaining: Option<Decimal>,
    pub agent_open_trades_remaining: Option<u32>,
    pub max_new_notional: Decimal,
}

/// Cached diagnostic headroom for a new intent. Admission always reads fresh evidence.
pub fn remaining_capacity(
    state: &AccountSafetyState,
    trades: &[Trade],
    subject: Option<&str>,
) -> Result<AdmissionCapacity> {
    let policy = state
        .policy
        .as_ref()
        .ok_or_else(|| failure("safety_policy_missing"))?;
    let snapshot = state
        .snapshot
        .as_ref()
        .ok_or_else(|| failure("broker_snapshot_missing"))?;
    let account_remaining = sub(
        policy.max_gross_notional,
        gross_commitment(snapshot, trades, None)?,
    )?
    .max(Decimal::ZERO);
    let reservations = trades
        .iter()
        .filter(|t| t.status == Status::Funded)
        .try_fold(Decimal::ZERO, |total, trade| {
            add(total, trade_notional(trade)?)
        })?;
    let available = sub(snapshot.available_funds, reservations)?.max(Decimal::ZERO);
    let mut capacity = AdmissionCapacity {
        currency: policy.currency.clone(),
        max_order_notional: policy.max_order_notional,
        account_notional_remaining: account_remaining,
        agent_notional_remaining: None,
        agent_open_trades_remaining: None,
        max_new_notional: policy
            .max_order_notional
            .min(account_remaining)
            .min(available),
    };
    if let Some(subject) = subject {
        let mandate = state
            .mandates
            .get(subject)
            .ok_or_else(|| failure("agent_mandate_missing"))?;
        let mut used = Decimal::ZERO;
        let mut count = 0u32;
        for trade in trades.iter().filter(|trade| {
            is_exposed(trade)
                && state
                    .trade_owners
                    .get(&trade.id)
                    .is_some_and(|owner| owner == subject)
        }) {
            used = add(used, marked_commitment(state, trade)?)?;
            count = count
                .checked_add(1)
                .ok_or_else(|| failure("risk_arithmetic_overflow"))?;
        }
        let remaining = sub(mandate.max_total_notional, used)?.max(Decimal::ZERO);
        let slots = mandate.max_open_trades.saturating_sub(count);
        capacity.max_order_notional = capacity.max_order_notional.min(mandate.max_order_notional);
        capacity.agent_notional_remaining = Some(remaining);
        capacity.agent_open_trades_remaining = Some(slots);
        capacity.max_new_notional = capacity
            .max_new_notional
            .min(capacity.max_order_notional)
            .min(remaining);
        if slots == 0 {
            capacity.max_new_notional = Decimal::ZERO;
        }
    }
    Ok(capacity)
}

pub fn admit(
    state: &AccountSafetyState,
    scope: &BrokerAccountScope,
    trade: &Trade,
    trades: &[Trade],
    subject: Option<&str>,
    now: DateTime<Utc>,
) -> Result<()> {
    if state.halt.is_some() {
        return Err(failure("account_halted"));
    }
    if !state.unresolved_operations.is_empty() {
        return Err(failure("unresolved_operations"));
    }
    let policy = state
        .policy
        .as_ref()
        .ok_or_else(|| failure("safety_policy_missing"))?;
    validate_policy(policy)?;
    let snapshot = state
        .snapshot
        .as_ref()
        .ok_or_else(|| failure("broker_snapshot_missing"))?;
    validate_snapshot(snapshot, scope, policy, now)?;
    check_equity(state, snapshot)?;
    verify_reconciled(snapshot, trades)?;
    if trade.currency.to_string().to_uppercase() != policy.currency {
        return Err(failure("risk_currency_mismatch"));
    }
    // Non-unit multiplier products need an explicit instrument-specific risk model.
    if !matches!(
        trade.trading_vehicle.category,
        model::TradingVehicleCategory::Stock
            | model::TradingVehicleCategory::Etf
            | model::TradingVehicleCategory::Crypto
    ) {
        return Err(failure("unsupported_risk_instrument"));
    }
    let proposed = trade_notional(trade)?;
    if proposed > policy.max_order_notional {
        return Err(failure("account_order_limit_exceeded"));
    }
    let gross = add(
        gross_commitment(snapshot, trades, Some(trade.id))?,
        proposed,
    )?;
    if gross > policy.max_gross_notional {
        return Err(failure("account_exposure_exceeded"));
    }
    let reserved = trades
        .iter()
        .filter(|t| t.status == Status::Funded && t.id != trade.id)
        .try_fold(Decimal::ZERO, |sum, t| add(sum, trade_notional(t)?))?;
    if add(reserved, proposed)? > snapshot.available_funds {
        return Err(failure("broker_capacity_exceeded"));
    }
    if let Some(subject) = subject {
        check_mandate(state, subject, trade, trades, now)?;
        if state
            .trade_owners
            .get(&trade.id)
            .is_none_or(|owner| owner != subject)
        {
            return Err(failure("trade_mandate_owner_mismatch"));
        }
    }
    Ok(())
}
