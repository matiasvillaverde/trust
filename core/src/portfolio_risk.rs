//! Deterministic prospective loss budgets. Every amount is in the broker base currency.
#![allow(missing_docs)]
use crate::{
    account_safety::{self, SafetyError},
    TrustFacade,
};
use chrono::{DateTime, Duration, Utc};
use model::{account_safety::*, portfolio_risk::*, Account, Status, Trade, TradeCategory};
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use serde::Serialize;
use std::{collections::BTreeMap, error::Error};
use uuid::Uuid;

type Result<T> = std::result::Result<T, SafetyError>;
fn fail(code: &str) -> SafetyError {
    SafetyError { code: code.into() }
}
fn add(a: Decimal, b: Decimal) -> Result<Decimal> {
    a.checked_add(b)
        .ok_or_else(|| fail("risk_arithmetic_overflow"))
}
fn sub(a: Decimal, b: Decimal) -> Result<Decimal> {
    a.checked_sub(b)
        .ok_or_else(|| fail("risk_arithmetic_overflow"))
}
fn mul(a: Decimal, b: Decimal) -> Result<Decimal> {
    let value = a
        .checked_mul(b)
        .ok_or_else(|| fail("risk_arithmetic_overflow"))?;
    if !a.is_zero() && !b.is_zero() && value.is_zero() {
        return Err(fail("risk_arithmetic_precision_loss"));
    }
    Ok(value)
}
fn div(a: Decimal, b: Decimal) -> Result<Decimal> {
    let value = a
        .checked_div(b)
        .ok_or_else(|| fail("risk_arithmetic_overflow"))?;
    if !a.is_zero() && value.is_zero() {
        return Err(fail("risk_arithmetic_precision_loss"));
    }
    Ok(value)
}
fn pct(a: Decimal, b: Decimal) -> Result<Decimal> {
    div(mul(a, b)?, dec!(100))
}
fn between(value: Decimal, low: Decimal, high: Decimal) -> bool {
    value >= low && value <= high
}

pub fn policy(state: &AccountSafetyState) -> Result<&PortfolioRiskPolicy> {
    state
        .policy
        .as_ref()
        .and_then(|p| p.controls.as_ref())
        .ok_or_else(|| fail("portfolio_policy_missing"))
}

pub fn validate_policy(p: &PortfolioRiskPolicy) -> Result<()> {
    for value in [
        p.max_portfolio_heat_pct,
        p.max_trade_heat_pct,
        p.max_stress_loss_pct,
        p.min_margin_cushion_pct,
        p.max_instrument_pct,
        p.max_sector_pct,
        p.max_cluster_pct,
        p.max_volume_participation_pct,
        p.max_book_participation_pct,
    ] {
        if !between(value, Decimal::ZERO, dec!(100)) {
            return Err(fail("invalid_portfolio_policy"));
        }
    }
    if p.max_trade_heat_pct > p.max_portfolio_heat_pct
        || p.max_instrument_pct > p.max_sector_pct
        || p.max_instrument_pct > p.max_cluster_pct
        || !(1..=30).contains(&p.max_quote_age_seconds)
        || !(60..=604800).contains(&p.max_history_age_seconds)
        || !(2..=60).contains(&p.volatility_lookback_bars)
        || !between(p.atr_multiplier, dec!(0.01), dec!(100))
        || !between(p.maintenance_margin_stress_pct, Decimal::ZERO, dec!(1000))
        || p.fee_per_unit < Decimal::ZERO
        || p.minimum_fee < Decimal::ZERO
        || p.drawdown_steps.is_empty()
        || p.drawdown_steps.len() > 20
        || p.instruments.is_empty()
        || p.instruments.len() > 500
        || !(60..=2592000).contains(&p.recovery_delay_seconds)
        || !between(p.recovery_step, dec!(0.01), Decimal::ONE)
    {
        return Err(fail("invalid_portfolio_policy"));
    }
    for bps in [p.max_spread_bps, p.max_entry_deviation_bps, p.slippage_bps] {
        if !between(bps, Decimal::ZERO, dec!(10000)) {
            return Err(fail("invalid_portfolio_policy"));
        }
    }
    let mut previous: Option<&DrawdownStep> = None;
    for step in &p.drawdown_steps {
        if !between(step.drawdown_pct, Decimal::ZERO, dec!(100))
            || !between(step.risk_multiplier, Decimal::ZERO, Decimal::ONE)
            || previous.is_none()
                && (step.drawdown_pct != Decimal::ZERO || step.risk_multiplier != Decimal::ONE)
            || previous.is_some_and(|old| {
                step.drawdown_pct <= old.drawdown_pct || step.risk_multiplier >= old.risk_multiplier
            })
        {
            return Err(fail("invalid_drawdown_steps"));
        }
        previous = Some(step);
    }
    for (id, profile) in &p.instruments {
        if id.is_empty()
            || id.len() > 128
            || profile.sector.trim().is_empty()
            || profile.sector.len() > 128
            || profile.correlation_group.trim().is_empty()
            || profile.correlation_group.len() > 128
            || !between(profile.gap_stress_pct, dec!(0.01), dec!(1000))
            || !between(profile.minimum_risk_pct, dec!(0.01), dec!(100))
            || profile.entry_windows.is_empty()
            || profile.entry_windows.len() > 64
            || profile.blackouts.len() > 64
        {
            return Err(fail("invalid_instrument_risk_policy"));
        }
        for window in profile.entry_windows.iter().chain(&profile.blackouts) {
            if window.starts_at >= window.ends_at || window.ends_at > profile.valid_until {
                return Err(fail("invalid_risk_window"));
            }
        }
    }
    Ok(())
}

pub fn validate_agent_budget(b: &AgentRiskBudget) -> Result<()> {
    if b.max_daily_loss <= Decimal::ZERO
        || b.max_daily_turnover <= Decimal::ZERO
        || !(1..=10000).contains(&b.max_daily_entries)
        || b.reentry_cooldown_seconds > 604800
    {
        return Err(fail("invalid_agent_risk_budget"));
    }
    Ok(())
}

fn snapshot(state: &AccountSafetyState) -> Result<&BrokerAccountSnapshot> {
    state
        .snapshot
        .as_ref()
        .ok_or_else(|| fail("broker_snapshot_missing"))
}
fn profile<'a>(
    p: &'a PortfolioRiskPolicy,
    id: &str,
    now: DateTime<Utc>,
) -> Result<&'a InstrumentRiskPolicy> {
    let instrument = p
        .instruments
        .get(id)
        .ok_or_else(|| fail("instrument_risk_policy_missing"))?;
    if instrument.valid_until <= now {
        return Err(fail("instrument_risk_policy_expired"));
    }
    Ok(instrument)
}
fn open_quantity(t: &Trade) -> Result<Decimal> {
    let q = sub(
        sub(t.entry.filled_quantity, t.safety_stop.filled_quantity)?,
        t.target.filled_quantity,
    )?;
    if q < Decimal::ZERO {
        return Err(fail("broker_ledger_disagreement"));
    }
    Ok(q)
}
fn instrument(t: &Trade) -> Result<&str> {
    t.trading_vehicle
        .broker_asset_id
        .as_deref()
        .filter(|id| !id.is_empty())
        .ok_or_else(|| fail("instrument_unresolved"))
}
fn mark(position: &BrokerPosition) -> Result<Decimal> {
    if position.quantity.is_zero() {
        return Err(fail("invalid_position_quantity"));
    }
    let price = div(position.market_value.abs(), position.quantity.abs())?;
    if price <= Decimal::ZERO {
        return Err(fail("invalid_position_price"));
    }
    Ok(price)
}
fn fee(p: &PortfolioRiskPolicy, q: Decimal) -> Result<Decimal> {
    Ok(mul(q, p.fee_per_unit)?.max(p.minimum_fee))
}
fn adverse_cost(p: &PortfolioRiskPolicy, price: Decimal) -> Result<Decimal> {
    div(mul(price, p.slippage_bps)?, dec!(10000))
}

pub fn drawdown_pct(state: &AccountSafetyState) -> Result<Decimal> {
    let base = state
        .baseline
        .as_ref()
        .ok_or_else(|| fail("equity_baseline_missing"))?;
    mul(
        div(
            sub(base.high_water, snapshot(state)?.equity)?.max(Decimal::ZERO),
            base.high_water,
        )?,
        dec!(100),
    )
}

/// Reduce immediately; restore at most one configured step per verified delay.
pub fn observe_throttle(state: &mut AccountSafetyState, now: DateTime<Utc>) -> Result<()> {
    let p = policy(state)?;
    let drawdown = drawdown_pct(state)?;
    let desired = p
        .drawdown_steps
        .iter()
        .rfind(|s| drawdown >= s.drawdown_pct)
        .map(|s| s.risk_multiplier)
        .ok_or_else(|| fail("invalid_drawdown_steps"))?;
    let delay =
        i64::try_from(p.recovery_delay_seconds).map_err(|_| fail("risk_arithmetic_overflow"))?;
    let step = p.recovery_step;
    match &mut state.portfolio.throttle {
        None => {
            state.portfolio.throttle = Some(DrawdownThrottle {
                multiplier: desired,
                changed_at: now,
            })
        }
        Some(t) if desired < t.multiplier => {
            t.multiplier = desired;
            t.changed_at = now;
        }
        Some(t)
            if desired > t.multiplier
                && now.signed_duration_since(t.changed_at).num_seconds() >= delay =>
        {
            t.multiplier = add(t.multiplier, step)?.min(desired);
            t.changed_at = now;
        }
        _ => {}
    }
    Ok(())
}

pub fn observe_agent_pnl(
    state: &mut AccountSafetyState,
    pnl: &BTreeMap<String, Decimal>,
    now: DateTime<Utc>,
) -> Result<()> {
    for (subject, value) in pnl {
        let loss = state
            .portfolio
            .agent_losses
            .entry(subject.clone())
            .or_insert(AgentLossState {
                day: now.date_naive(),
                // Historical gains must not grant fresh daily loss capacity.
                day_start_pnl: (*value).max(Decimal::ZERO),
                last_pnl: Decimal::ZERO,
            });
        if now.date_naive() < loss.day {
            return Err(fail("risk_clock_regressed"));
        }
        if now.date_naive() > loss.day {
            loss.day = now.date_naive();
            loss.day_start_pnl = loss.last_pnl;
        }
        loss.last_pnl = *value;
    }
    Ok(())
}

/// Observe executed entry/exit notional once; late first observations charge the current day.
pub fn observe_turnover(
    state: &mut AccountSafetyState,
    trades: &[Trade],
    now: DateTime<Utc>,
) -> Result<()> {
    for t in trades
        .iter()
        .filter(|t| state.trade_owners.contains_key(&t.id))
    {
        let mut cumulative = Decimal::ZERO;
        for leg in [&t.entry, &t.safety_stop, &t.target] {
            if leg.filled_quantity < Decimal::ZERO {
                return Err(fail("agent_fill_evidence_invalid"));
            }
            if leg.filled_quantity > Decimal::ZERO {
                let price = leg
                    .average_filled_price
                    .filter(|p| *p > Decimal::ZERO)
                    .ok_or_else(|| fail("agent_fill_evidence_missing"))?;
                cumulative = add(cumulative, mul(leg.filled_quantity, price)?)?;
            }
        }
        let observed = state
            .portfolio
            .turnover
            .entry(t.id)
            .or_insert(TradeTurnover {
                day: now.date_naive(),
                cumulative: Decimal::ZERO,
                today: Decimal::ZERO,
            });
        if observed.day > now.date_naive() || cumulative < observed.cumulative {
            return Err(fail("agent_turnover_evidence_regressed"));
        }
        if observed.day < now.date_naive() {
            observed.day = now.date_naive();
            observed.today = Decimal::ZERO;
        }
        observed.today = add(observed.today, sub(cumulative, observed.cumulative)?)?;
        observed.cumulative = cumulative;
    }
    Ok(())
}

fn activity_usage(
    state: &AccountSafetyState,
    trades: &[Trade],
    subject: &str,
    now: DateTime<Utc>,
) -> Result<(Decimal, u32)> {
    let mut charged = BTreeMap::<Uuid, Decimal>::new();
    let mut count = 0u32;
    for charge in state
        .portfolio
        .entry_charges
        .iter()
        .filter(|c| c.subject == subject)
    {
        if charge.at > now {
            return Err(fail("risk_clock_regressed"));
        }
        let pending = state.portfolio.reservations.contains_key(&charge.trade_id)
            && trades.iter().any(|t| {
                t.id == charge.trade_id
                    && matches!(
                        t.status,
                        Status::New
                            | Status::Funded
                            | Status::Submitted
                            | Status::PartiallyFilled
                            | Status::Filled
                    )
            });
        if charge.at.date_naive() == now.date_naive() || pending {
            charged.insert(charge.trade_id, charge.turnover);
            count = count
                .checked_add(1)
                .ok_or_else(|| fail("risk_arithmetic_overflow"))?;
        }
    }
    for (id, r) in state
        .portfolio
        .reservations
        .iter()
        .filter(|(_, r)| r.subject == subject)
    {
        if !charged.contains_key(id)
            && trades.iter().any(|t| {
                t.id == *id
                    && matches!(
                        t.status,
                        Status::New
                            | Status::Funded
                            | Status::Submitted
                            | Status::PartiallyFilled
                            | Status::Filled
                    )
            })
        {
            charged.insert(*id, mul(r.notional, dec!(2))?);
            count = count
                .checked_add(1)
                .ok_or_else(|| fail("risk_arithmetic_overflow"))?;
        }
    }
    for (id, observed) in &state.portfolio.turnover {
        if observed.day == now.date_naive()
            && state
                .trade_owners
                .get(id)
                .is_some_and(|owner| owner == subject)
        {
            let usage = charged.entry(*id).or_default();
            *usage = (*usage).max(observed.today);
        }
    }
    Ok((
        charged
            .values()
            .try_fold(Decimal::ZERO, |total, v| add(total, *v))?,
        count,
    ))
}

/// Readiness for a new agent intent; risk-reducing actions remain available.
pub fn check_agent_capacity(
    state: &AccountSafetyState,
    trades: &[Trade],
    subject: &str,
    now: DateTime<Utc>,
) -> Result<()> {
    let budget = state
        .mandates
        .get(subject)
        .and_then(|m| m.risk_budget.as_ref())
        .ok_or_else(|| fail("agent_risk_budget_missing"))?;
    validate_agent_budget(budget)?;
    let loss = state
        .portfolio
        .agent_losses
        .get(subject)
        .ok_or_else(|| fail("agent_loss_evidence_missing"))?;
    if loss.day != now.date_naive() {
        return Err(fail("agent_loss_evidence_stale"));
    }
    if sub(loss.day_start_pnl, loss.last_pnl)? >= budget.max_daily_loss {
        return Err(fail("agent_daily_loss_exceeded"));
    }
    let (used, count) = activity_usage(state, trades, subject, now)?;
    if used >= budget.max_daily_turnover {
        return Err(fail("agent_turnover_exhausted"));
    }
    if count >= budget.max_daily_entries {
        return Err(fail("agent_entries_exhausted"));
    }
    Ok(())
}

#[derive(Debug, Clone, Serialize, schemars::JsonSchema)]
pub struct PortfolioRiskReport {
    pub currency: String,
    pub observed_at: DateTime<Utc>,
    pub equity: Decimal,
    pub planned_loss: Decimal,
    pub heat_limit: Decimal,
    pub heat_remaining: Decimal,
    pub stress_loss: Decimal,
    pub stress_limit: Decimal,
    pub stress_remaining: Decimal,
    pub margin_cushion_after_stress: Decimal,
    pub required_margin_cushion: Decimal,
    pub drawdown_pct: Decimal,
    pub sizing_multiplier: Decimal,
    pub instrument_notional: BTreeMap<String, Decimal>,
    pub sector_notional: BTreeMap<String, Decimal>,
    pub cluster_notional: BTreeMap<String, Decimal>,
}

#[derive(Default)]
struct Totals {
    heat: Decimal,
    stress: Decimal,
    pending_notional: Decimal,
    instruments: BTreeMap<String, Decimal>,
    sectors: BTreeMap<String, Decimal>,
    clusters: BTreeMap<String, Decimal>,
}
impl Totals {
    fn record(
        &mut self,
        id: &str,
        profile: &InstrumentRiskPolicy,
        notional: Decimal,
        heat: Decimal,
        stress: Decimal,
        pending: bool,
    ) -> Result<()> {
        self.heat = add(self.heat, heat)?;
        self.stress = add(self.stress, stress)?;
        if pending {
            self.pending_notional = add(self.pending_notional, notional)?;
        }
        for (map, key) in [
            (&mut self.instruments, id),
            (&mut self.sectors, profile.sector.as_str()),
            (&mut self.clusters, profile.correlation_group.as_str()),
        ] {
            let value = add(map.get(key).copied().unwrap_or(Decimal::ZERO), notional)?;
            map.insert(key.into(), value);
        }
        Ok(())
    }
}

fn modeled_loss(
    p: &PortfolioRiskPolicy,
    profile: &InstrumentRiskPolicy,
    q: Decimal,
    price: Decimal,
    stop: Option<Decimal>,
    short: bool,
    floor: Decimal,
) -> Result<(Decimal, Decimal)> {
    let distance = match stop {
        Some(stop) if stop > Decimal::ZERO => if short {
            sub(stop, price)?
        } else {
            sub(price, stop)?
        }
        .max(Decimal::ZERO),
        _ if short => return Err(fail("unprotected_short_exposure")),
        _ => price,
    };
    let unit = add(
        distance
            .max(pct(price, profile.minimum_risk_pct)?)
            .max(floor),
        adverse_cost(p, price)?,
    )?;
    let costs = fee(p, q)?;
    let heat = add(mul(q, unit)?, costs)?;
    let stress = add(
        mul(q, pct(price, profile.gap_stress_pct)?.max(unit))?,
        costs,
    )?;
    Ok((heat, stress))
}

fn protective_stop(snapshot: &BrokerAccountSnapshot, t: &Trade, q: Decimal) -> Option<Decimal> {
    snapshot
        .working_orders
        .iter()
        .find(|o| {
            account_safety::order_matches(o, &t.safety_stop)
                && o.active
                && o.order_type == "stop"
                && o.remaining_quantity >= q
                && o.instrument_id == t.trading_vehicle.broker_asset_id.as_deref().unwrap_or("")
                && o.side
                    == if t.category == TradeCategory::Short {
                        "buy"
                    } else {
                        "sell"
                    }
        })
        .and_then(|o| o.stop_price)
}

fn record_position(
    total: &mut Totals,
    state: &AccountSafetyState,
    trades: &[Trade],
    position: &BrokerPosition,
    now: DateTime<Utc>,
) -> Result<()> {
    let p = policy(state)?;
    let s = snapshot(state)?;
    let profile = profile(p, &position.instrument_id, now)?;
    let price = mark(position)?;
    let mut remaining = position.quantity.abs();
    for t in trades.iter().filter(|t| {
        matches!(
            t.status,
            Status::Filled | Status::PartiallyFilled | Status::Submitted
        ) && t.trading_vehicle.broker_asset_id.as_deref() == Some(position.instrument_id.as_str())
    }) {
        let q = open_quantity(t)?;
        if q.is_zero() {
            continue;
        }
        if (t.category == TradeCategory::Short) != (position.quantity < Decimal::ZERO)
            || q > remaining
        {
            return Err(fail("opposing_or_unreconciled_positions"));
        }
        remaining = sub(remaining, q)?;
        let floor = state
            .portfolio
            .reservations
            .get(&t.id)
            .map(|r| r.minimum_unit_risk)
            .unwrap_or(Decimal::ZERO);
        let (heat, stress) = modeled_loss(
            p,
            profile,
            q,
            price,
            protective_stop(s, t, q),
            t.category == TradeCategory::Short,
            floor,
        )?;
        total.record(
            &position.instrument_id,
            profile,
            mul(q, price)?,
            heat,
            stress,
            false,
        )?;
    }
    record_manual_position(total, state, trades, position, remaining, now)
}
fn record_manual_position(
    total: &mut Totals,
    state: &AccountSafetyState,
    trades: &[Trade],
    position: &BrokerPosition,
    remaining: Decimal,
    now: DateTime<Utc>,
) -> Result<()> {
    let p = policy(state)?;
    let s = snapshot(state)?;
    let profile = profile(p, &position.instrument_id, now)?;
    let price = mark(position)?;
    if remaining > Decimal::ZERO {
        // Manual exposure receives protection credit only for an unambiguous full-position stop.
        let stop = s
            .working_orders
            .iter()
            .find(|o| {
                o.instrument_id == position.instrument_id
                    && account_safety::verified_manual_stop(o, s, trades)
            })
            .and_then(|o| o.stop_price);
        let (heat, stress) = modeled_loss(
            p,
            profile,
            remaining,
            price,
            stop,
            position.quantity < Decimal::ZERO,
            Decimal::ZERO,
        )?;
        total.record(
            &position.instrument_id,
            profile,
            mul(remaining, price)?,
            heat,
            stress,
            false,
        )?;
    }
    Ok(())
}
fn record_working_orders(
    total: &mut Totals,
    state: &AccountSafetyState,
    trades: &[Trade],
    now: DateTime<Utc>,
) -> Result<()> {
    let p = policy(state)?;
    let s = snapshot(state)?;
    for order in s
        .working_orders
        .iter()
        .filter(|o| o.remaining_quantity > Decimal::ZERO)
    {
        if account_safety::verified_exit(order, s, trades) {
            continue;
        }
        let profile = profile(p, &order.instrument_id, now)?;
        let n = order
            .notional
            .filter(|v| *v > Decimal::ZERO)
            .ok_or_else(|| fail("unbounded_working_order"))?;
        let price = div(n, order.remaining_quantity)?;
        let t = trades
            .iter()
            .find(|t| account_safety::order_matches(order, &t.entry));
        let stop = t.map(|t| t.safety_stop.unit_price);
        let floor = t
            .and_then(|t| state.portfolio.reservations.get(&t.id))
            .map(|r| r.minimum_unit_risk)
            .unwrap_or(Decimal::ZERO);
        let (heat, stress) = modeled_loss(
            p,
            profile,
            order.remaining_quantity,
            price,
            stop,
            order.side == "sell",
            floor,
        )?;
        total.record(&order.instrument_id, profile, n, heat, stress, true)?;
    }
    Ok(())
}

fn totals(
    state: &AccountSafetyState,
    trades: &[Trade],
    excluding: Option<Uuid>,
    now: DateTime<Utc>,
) -> Result<Totals> {
    let p = policy(state)?;
    let s = snapshot(state)?;
    account_safety::verify_reconciled(s, trades)?;
    let mut totals = Totals::default();
    for position in s.positions.iter().filter(|p| !p.quantity.is_zero()) {
        record_position(&mut totals, state, trades, position, now)?;
    }
    record_working_orders(&mut totals, state, trades, now)?;
    for t in trades.iter().filter(|t| {
        Some(t.id) != excluding
            && (t.status == Status::Funded
                || t.status == Status::New && state.portfolio.reservations.contains_key(&t.id))
    }) {
        let id = instrument(t)?;
        let profile = profile(p, id, now)?;
        let floor = state
            .portfolio
            .reservations
            .get(&t.id)
            .map(|r| r.minimum_unit_risk)
            .unwrap_or(Decimal::ZERO);
        let (heat, stress) = modeled_loss(
            p,
            profile,
            t.entry.quantity,
            t.entry.unit_price,
            Some(t.safety_stop.unit_price),
            t.category == TradeCategory::Short,
            floor,
        )?;
        totals.record(
            id,
            profile,
            account_safety::trade_notional(t)?,
            heat,
            stress,
            true,
        )?;
    }
    Ok(totals)
}

fn report(
    state: &AccountSafetyState,
    total: Totals,
    now: DateTime<Utc>,
) -> Result<PortfolioRiskReport> {
    let p = policy(state)?;
    let s = snapshot(state)?;
    let base = state
        .baseline
        .as_ref()
        .ok_or_else(|| fail("equity_baseline_missing"))?;
    let owner = state
        .policy
        .as_ref()
        .ok_or_else(|| fail("safety_policy_missing"))?;
    let margin = s
        .margin
        .as_ref()
        .ok_or_else(|| fail("margin_evidence_missing"))?;
    if margin.observed_at > now
        || now.signed_duration_since(margin.observed_at).num_seconds()
            > i64::try_from(owner.max_snapshot_age_seconds)
                .map_err(|_| fail("risk_arithmetic_overflow"))?
        || margin.maintenance_margin < Decimal::ZERO
    {
        return Err(fail("margin_evidence_invalid_or_stale"));
    }
    let daily_remaining = sub(
        owner.max_daily_loss,
        sub(base.day_start, s.equity)?.max(Decimal::ZERO),
    )?
    .max(Decimal::ZERO);
    let drawdown_remaining = sub(
        owner.max_drawdown,
        sub(base.high_water, s.equity)?.max(Decimal::ZERO),
    )?
    .max(Decimal::ZERO);
    let heat_limit = pct(s.equity, p.max_portfolio_heat_pct)?
        .min(daily_remaining)
        .min(drawdown_remaining);
    let stress_limit = pct(s.equity, p.max_stress_loss_pct)?;
    let margin_after = sub(
        sub(
            sub(margin.excess_liquidity, total.stress)?,
            total.pending_notional,
        )?,
        pct(margin.maintenance_margin, p.maintenance_margin_stress_pct)?,
    )?;
    Ok(PortfolioRiskReport {
        currency: s.currency.clone(),
        observed_at: s.observed_at.min(margin.observed_at),
        equity: s.equity,
        planned_loss: total.heat,
        heat_limit,
        heat_remaining: sub(heat_limit, total.heat)?.max(Decimal::ZERO),
        stress_loss: total.stress,
        stress_limit,
        stress_remaining: sub(stress_limit, total.stress)?.max(Decimal::ZERO),
        margin_cushion_after_stress: margin_after,
        required_margin_cushion: pct(s.equity, p.min_margin_cushion_pct)?,
        drawdown_pct: drawdown_pct(state)?,
        sizing_multiplier: state
            .portfolio
            .throttle
            .as_ref()
            .map(|t| t.multiplier)
            .ok_or_else(|| fail("drawdown_observation_missing"))?,
        instrument_notional: total.instruments,
        sector_notional: total.sectors,
        cluster_notional: total.clusters,
    })
}

fn check_report(p: &PortfolioRiskPolicy, r: &PortfolioRiskReport) -> Result<()> {
    if r.planned_loss > r.heat_limit {
        return Err(fail("portfolio_heat_exceeded"));
    }
    if r.stress_loss > r.stress_limit {
        return Err(fail("portfolio_stress_exceeded"));
    }
    if r.margin_cushion_after_stress < r.required_margin_cushion {
        return Err(fail("margin_cushion_exhausted"));
    }
    for (map, cap, code) in [
        (
            &r.instrument_notional,
            p.max_instrument_pct,
            "instrument_concentration_exceeded",
        ),
        (
            &r.sector_notional,
            p.max_sector_pct,
            "sector_concentration_exceeded",
        ),
        (
            &r.cluster_notional,
            p.max_cluster_pct,
            "cluster_concentration_exceeded",
        ),
    ] {
        let limit = pct(r.equity, cap)?;
        if map.values().any(|value| *value > limit) {
            return Err(fail(code));
        }
    }
    Ok(())
}

pub fn portfolio_report(
    state: &AccountSafetyState,
    trades: &[Trade],
    now: DateTime<Utc>,
) -> Result<PortfolioRiskReport> {
    validate_policy(policy(state)?)?;
    let s = snapshot(state)?;
    account_safety::validate_snapshot(
        s,
        &s.scope,
        state
            .policy
            .as_ref()
            .ok_or_else(|| fail("safety_policy_missing"))?,
        now,
    )?;
    if state
        .portfolio
        .reservations
        .keys()
        .any(|id| !trades.iter().any(|t| t.id == *id))
    {
        return Err(fail("risk_reservation_unreconciled"));
    }
    report(state, totals(state, trades, None, now)?, now)
}
pub fn check_existing(
    state: &AccountSafetyState,
    trades: &[Trade],
    now: DateTime<Utc>,
) -> Result<PortfolioRiskReport> {
    let result = portfolio_report(state, trades, now)?;
    check_report(policy(state)?, &result)?;
    Ok(result)
}

pub fn market_unit_floor(
    p: &PortfolioRiskPolicy,
    s: &BrokerAccountSnapshot,
    t: &Trade,
    data: &RiskMarketData,
    now: DateTime<Utc>,
) -> Result<Decimal> {
    if data.scope != s.scope
        || data.instrument_id != instrument(t)?
        || !data.real_time
        || data.observed_at > data.received_at
        || data.received_at > now
        || data.observed_at > now
        || now.signed_duration_since(data.observed_at).num_seconds()
            > i64::try_from(p.max_quote_age_seconds)
                .map_err(|_| fail("risk_arithmetic_overflow"))?
        || data.bid <= Decimal::ZERO
        || data.ask < data.bid
        || data.bid_size <= Decimal::ZERO
        || data.ask_size <= Decimal::ZERO
    {
        return Err(fail("market_evidence_invalid_or_stale"));
    }
    let mid = div(add(data.bid, data.ask)?, dec!(2))?;
    if mul(div(sub(data.ask, data.bid)?, mid)?, dec!(10000))? > p.max_spread_bps {
        return Err(fail("spread_limit_exceeded"));
    }
    if mul(div(sub(t.entry.unit_price, mid)?.abs(), mid)?, dec!(10000))? > p.max_entry_deviation_bps
    {
        return Err(fail("entry_price_collar_exceeded"));
    }
    let lookback = usize::try_from(p.volatility_lookback_bars)
        .map_err(|_| fail("risk_arithmetic_overflow"))?;
    let required = lookback
        .checked_add(1)
        .ok_or_else(|| fail("risk_arithmetic_overflow"))?;
    if data.daily_bars.len() < required || data.daily_bars.len() > 512 {
        return Err(fail("volatility_history_incomplete"));
    }
    let mut previous = None;
    for bar in &data.daily_bars {
        if bar.time.date_naive() >= now.date_naive()
            || bar.open <= Decimal::ZERO
            || bar.close <= Decimal::ZERO
            || bar.low <= Decimal::ZERO
            || bar.high < bar.open.max(bar.close)
            || bar.low > bar.open.min(bar.close)
            || previous.is_some_and(|time| bar.time <= time)
        {
            return Err(fail("volatility_history_invalid"));
        }
        previous = Some(bar.time);
    }
    let latest = data
        .daily_bars
        .last()
        .ok_or_else(|| fail("volatility_history_incomplete"))?;
    if now.signed_duration_since(latest.time).num_seconds()
        > i64::try_from(p.max_history_age_seconds).map_err(|_| fail("risk_arithmetic_overflow"))?
    {
        return Err(fail("volatility_history_stale"));
    }
    let pairs = data.daily_bars.windows(2).rev().take(lookback);
    let mut range = Decimal::ZERO;
    for pair in pairs {
        let previous = pair
            .first()
            .ok_or_else(|| fail("volatility_history_incomplete"))?;
        let bar = pair
            .last()
            .ok_or_else(|| fail("volatility_history_incomplete"))?;
        range = add(
            range,
            sub(bar.high, bar.low)?
                .max(sub(bar.high, previous.close)?.abs())
                .max(sub(bar.low, previous.close)?.abs()),
        )?;
    }
    mul(
        div(range, Decimal::from(p.volatility_lookback_bars))?,
        p.atr_multiplier,
    )
}

fn check_activity(
    state: &AccountSafetyState,
    t: &Trade,
    trades: &[Trade],
    subject: Option<&str>,
    turnover: Decimal,
    now: DateTime<Utc>,
) -> Result<()> {
    let Some(subject) = subject else {
        return Ok(());
    };
    let mandate = state
        .mandates
        .get(subject)
        .ok_or_else(|| fail("agent_mandate_missing"))?;
    let budget = mandate
        .risk_budget
        .as_ref()
        .ok_or_else(|| fail("agent_risk_budget_missing"))?;
    validate_agent_budget(budget)?;
    let loss = state
        .portfolio
        .agent_losses
        .get(subject)
        .ok_or_else(|| fail("agent_loss_evidence_missing"))?;
    if sub(loss.day_start_pnl, loss.last_pnl)? >= budget.max_daily_loss {
        return Err(fail("agent_daily_loss_exceeded"));
    }
    let existing = t.status == Status::Funded && state.portfolio.reservations.contains_key(&t.id);
    let (mut used, mut count) = activity_usage(state, trades, subject, now)?;
    let cooldown = i64::try_from(budget.reentry_cooldown_seconds)
        .map_err(|_| fail("risk_arithmetic_overflow"))?;
    if !existing {
        let instrument_id = instrument(t)?;
        for charge in state
            .portfolio
            .entry_charges
            .iter()
            .filter(|c| c.subject == subject && c.instrument_id == instrument_id)
        {
            if now.signed_duration_since(charge.at).num_seconds() < cooldown {
                return Err(fail("agent_reentry_cooldown"));
            }
        }
        for prior in trades.iter().filter(|prior| {
            prior.id != t.id
                && state
                    .trade_owners
                    .get(&prior.id)
                    .is_some_and(|owner| owner == subject)
                && prior.trading_vehicle.broker_asset_id == t.trading_vehicle.broker_asset_id
                && matches!(prior.status, Status::ClosedTarget | Status::ClosedStopLoss)
        }) {
            if now
                .signed_duration_since(prior.updated_at.and_utc())
                .num_seconds()
                < cooldown
            {
                return Err(fail("agent_reentry_cooldown"));
            }
        }
    }
    if !existing {
        used = add(used, turnover)?;
        count = count
            .checked_add(1)
            .ok_or_else(|| fail("risk_arithmetic_overflow"))?;
    }
    if used > budget.max_daily_turnover {
        return Err(fail("agent_turnover_exceeded"));
    }
    if count > budget.max_daily_entries {
        return Err(fail("agent_entry_count_exceeded"));
    }
    Ok(())
}

#[derive(Debug, Clone, Serialize, schemars::JsonSchema)]
pub struct AdmissionRisk {
    pub proposed_quantity: Decimal,
    pub proposed_planned_loss: Decimal,
    pub proposed_stress_loss: Decimal,
    /// Conservative volatility floor in base currency per instrument unit.
    pub minimum_unit_risk: Decimal,
    pub market_observed_at: DateTime<Utc>,
    pub history_end_at: DateTime<Utc>,
    pub portfolio: PortfolioRiskReport,
}

fn check_liquidity(
    state: &AccountSafetyState,
    t: &Trade,
    trades: &[Trade],
    data: &RiskMarketData,
) -> Result<()> {
    let p = policy(state)?;
    let s = snapshot(state)?;
    let id = instrument(t)?;
    let q = t.entry.quantity;
    if q <= Decimal::ZERO {
        return Err(fail("invalid_quantity"));
    }
    let mut committed = q;
    for order in s
        .working_orders
        .iter()
        .filter(|o| o.instrument_id == id && o.remaining_quantity > Decimal::ZERO)
    {
        if !account_safety::verified_exit(order, s, trades)
            && !account_safety::order_matches(order, &t.entry)
        {
            committed = add(committed, order.remaining_quantity)?;
        }
    }
    for other in trades.iter().filter(|other| {
        other.id != t.id
            && other.trading_vehicle.broker_asset_id.as_deref() == Some(id)
            && (other.status == Status::Funded
                || other.status == Status::New
                    && state.portfolio.reservations.contains_key(&other.id))
    }) {
        committed = add(committed, other.entry.quantity)?;
    }
    let volume = data
        .daily_bars
        .last()
        .map(|b| Decimal::from(b.volume))
        .ok_or_else(|| fail("volatility_history_incomplete"))?;
    let depth = if t.category == TradeCategory::Short {
        data.bid_size
    } else {
        data.ask_size
    };
    if committed > pct(volume, p.max_volume_participation_pct)?
        || committed > pct(depth, p.max_book_participation_pct)?
    {
        return Err(fail("liquidity_participation_exceeded"));
    }
    Ok(())
}

fn entry_profile<'a>(
    p: &'a PortfolioRiskPolicy,
    id: &str,
    now: DateTime<Utc>,
) -> Result<&'a InstrumentRiskPolicy> {
    let ip = profile(p, id, now)?;
    if !ip
        .entry_windows
        .iter()
        .any(|w| now >= w.starts_at && now < w.ends_at)
        || ip
            .blackouts
            .iter()
            .any(|w| now >= w.starts_at && now < w.ends_at)
    {
        return Err(fail("entry_window_blocked"));
    }
    Ok(ip)
}

pub fn admit(
    state: &AccountSafetyState,
    t: &Trade,
    trades: &[Trade],
    subject: Option<&str>,
    now: DateTime<Utc>,
) -> Result<AdmissionRisk> {
    let p = policy(state)?;
    validate_policy(p)?;
    let s = snapshot(state)?;
    let id = instrument(t)?;
    let ip = entry_profile(p, id, now)?;
    let data = state
        .portfolio
        .market
        .get(id)
        .ok_or_else(|| fail("market_evidence_missing"))?;
    let floor = market_unit_floor(p, s, t, data, now)?.max(
        state
            .portfolio
            .reservations
            .get(&t.id)
            .map(|r| r.minimum_unit_risk)
            .unwrap_or(Decimal::ZERO),
    );
    let q = t.entry.quantity;
    check_liquidity(state, t, trades, data)?;
    let (heat, stress) = modeled_loss(
        p,
        ip,
        q,
        t.entry.unit_price,
        Some(t.safety_stop.unit_price),
        t.category == TradeCategory::Short,
        floor,
    )?;
    let n = account_safety::trade_notional(t)?;
    let mut total = totals(state, trades, Some(t.id), now)?;
    total.record(id, ip, n, heat, stress, true)?;
    let report = report(state, total, now)?;
    if report.sizing_multiplier <= Decimal::ZERO {
        return Err(fail("drawdown_blocks_new_risk"));
    }
    if heat
        > mul(
            pct(s.equity, p.max_trade_heat_pct)?,
            report.sizing_multiplier,
        )?
    {
        return Err(fail("trade_heat_exceeded"));
    }
    let owner = state
        .policy
        .as_ref()
        .ok_or_else(|| fail("safety_policy_missing"))?;
    if n > mul(owner.max_order_notional, report.sizing_multiplier)? {
        return Err(fail("drawdown_size_exceeded"));
    }
    check_report(p, &report)?;
    check_activity(state, t, trades, subject, mul(n, dec!(2))?, now)?;
    Ok(AdmissionRisk {
        proposed_quantity: q,
        proposed_planned_loss: heat,
        proposed_stress_loss: stress,
        minimum_unit_risk: floor,
        market_observed_at: data.observed_at,
        history_end_at: data
            .daily_bars
            .last()
            .ok_or_else(|| fail("volatility_history_incomplete"))?
            .time,
        portfolio: report,
    })
}

/// Called after admission but before financial mutation, in the serialized worker.
pub fn reserve(
    state: &mut AccountSafetyState,
    t: &Trade,
    subject: &str,
    request: Uuid,
    risk: &AdmissionRisk,
    now: DateTime<Utc>,
) -> Result<()> {
    if t.status == Status::Funded {
        if let Some(existing) = state.portfolio.reservations.get_mut(&t.id) {
            existing.minimum_unit_risk = existing.minimum_unit_risk.max(risk.minimum_unit_risk);
            existing.planned_loss = existing.planned_loss.max(risk.proposed_planned_loss);
            existing.stress_loss = existing.stress_loss.max(risk.proposed_stress_loss);
            return Ok(());
        }
    }
    let n = account_safety::trade_notional(t)?;
    state.portfolio.reservations.insert(
        t.id,
        RiskReservation {
            request_id: request,
            subject: subject.into(),
            instrument_id: instrument(t)?.into(),
            created_at: now,
            planned_loss: risk.proposed_planned_loss,
            stress_loss: risk.proposed_stress_loss,
            notional: n,
            minimum_unit_risk: risk.minimum_unit_risk,
        },
    );
    state.portfolio.entry_charges.push(EntryCharge {
        request_id: request,
        trade_id: t.id,
        subject: subject.into(),
        instrument_id: instrument(t)?.into(),
        at: now,
        turnover: mul(n, dec!(2))?,
    });
    Ok(())
}

pub fn release_definitive_rejection(state: &mut AccountSafetyState, request: Uuid) {
    state
        .portfolio
        .reservations
        .retain(|_, r| r.request_id != request);
    state
        .portfolio
        .entry_charges
        .retain(|c| c.request_id != request);
}

pub fn reconcile_reservations(
    state: &mut AccountSafetyState,
    trades: &[Trade],
    now: DateTime<Utc>,
) {
    if state.unresolved_operations.is_empty() {
        state.portfolio.reservations.retain(|id, _| {
            !trades.iter().any(|t| {
                t.id == *id
                    && matches!(
                        t.status,
                        Status::ClosedTarget
                            | Status::ClosedStopLoss
                            | Status::Canceled
                            | Status::Expired
                            | Status::Rejected
                    )
            })
        });
    }
    let cutoff = now.checked_sub_signed(Duration::days(8));
    state
        .portfolio
        .entry_charges
        .retain(|c| cutoff.is_none_or(|cutoff| c.at >= cutoff));
}

impl TrustFacade {
    pub fn validate_risk_proposal(
        &mut self,
        trade: &Trade,
    ) -> std::result::Result<(), Box<dyn Error>> {
        crate::validators::funding::can_fund(trade, self.factory.as_mut())
            .map_err(|e| e as Box<dyn Error>)
    }
    pub fn broker_risk_market_data(
        &self,
        account: &Account,
        instrument: &str,
        lookback: u32,
    ) -> std::result::Result<RiskMarketData, Box<dyn Error>> {
        self.broker.risk_market_data(account, instrument, lookback)
    }

    /// Revalue actual fills and fees, not the possibly stale trade balance projection.
    pub fn agent_risk_pnl(
        &mut self,
        state: &AccountSafetyState,
        trades: &[Trade],
    ) -> std::result::Result<BTreeMap<String, Decimal>, Box<dyn Error>> {
        let mut result: BTreeMap<String, Decimal> = state
            .mandates
            .keys()
            .map(|s| (s.clone(), Decimal::ZERO))
            .collect();
        let s = snapshot(state)?;
        for t in trades {
            let Some(subject) = state.trade_owners.get(&t.id) else {
                continue;
            };
            if t.currency.to_string().to_uppercase() != s.currency {
                return Err(Box::new(fail("risk_currency_mismatch")));
            }
            let mut value = Decimal::ZERO;
            for (leg, entry) in [
                (&t.entry, true),
                (&t.safety_stop, false),
                (&t.target, false),
            ] {
                if leg.filled_quantity > Decimal::ZERO {
                    let price = leg
                        .average_filled_price
                        .filter(|p| *p > Decimal::ZERO)
                        .ok_or_else(|| fail("agent_fill_evidence_missing"))?;
                    let cash = mul(leg.filled_quantity, price)?;
                    value = if entry == (t.category == TradeCategory::Long) {
                        sub(value, cash)?
                    } else {
                        add(value, cash)?
                    };
                }
            }
            let q = open_quantity(t)?;
            if q > Decimal::ZERO {
                let id = instrument(t)?;
                let pos = s
                    .positions
                    .iter()
                    .find(|p| p.instrument_id == id)
                    .ok_or_else(|| fail("agent_position_evidence_missing"))?;
                let remaining = mul(q, mark(pos)?)?;
                value = if t.category == TradeCategory::Long {
                    add(value, remaining)?
                } else {
                    sub(value, remaining)?
                };
            }
            for tx in self
                .factory
                .transaction_read()
                .all_trade_transactions(t.id)?
            {
                if matches!(
                    tx.category,
                    model::TransactionCategory::FeeOpen(_)
                        | model::TransactionCategory::FeeClose(_)
                        | model::TransactionCategory::PaymentTax(_)
                ) {
                    value = sub(value, tx.amount)?;
                }
            }
            let total = add(result.get(subject).copied().unwrap_or(Decimal::ZERO), value)?;
            result.insert(subject.clone(), total);
        }
        Ok(result)
    }
}
