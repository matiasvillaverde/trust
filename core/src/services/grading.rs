#![allow(
    missing_docs,
    missing_debug_implementations,
    clippy::arithmetic_side_effects,
    clippy::cast_sign_loss,
    clippy::field_reassign_with_default,
    clippy::indexing_slicing,
    clippy::too_many_lines,
    clippy::type_complexity
)]

use chrono::{Duration, NaiveDateTime, TimeZone, Utc};
use model::{
    Account, BarTimeframe, Broker, DatabaseFactory, Grade, MarketBar, OrderCategory, RuleName,
    Status, Trade, TradeGrade,
};
use rust_decimal::prelude::ToPrimitive;
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use std::error::Error;
use uuid::Uuid;

/// Grading weights in permille (sum must be 1000).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GradingWeightsPermille {
    pub process: u16,
    pub risk: u16,
    pub execution: u16,
    pub documentation: u16,
}

impl Default for GradingWeightsPermille {
    fn default() -> Self {
        Self {
            process: 400,
            risk: 300,
            execution: 200,
            documentation: 100,
        }
    }
}

impl GradingWeightsPermille {
    pub fn validate(&self) -> Result<(), Box<dyn Error>> {
        let sum: u16 = self
            .process
            .saturating_add(self.risk)
            .saturating_add(self.execution)
            .saturating_add(self.documentation);
        if sum != 1000 {
            return Err(
                format!("Invalid grading weights: expected sum=1000 permille, got {sum}").into(),
            );
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct WeightedPointsBreakdown {
    pub process_points: Decimal,
    pub risk_points: Decimal,
    pub execution_points: Decimal,
    pub documentation_points: Decimal,
    pub total_points: Decimal,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MarketDataDetails {
    pub status: MarketDataStatus,
    pub timeframe: Option<BarTimeframe>,
    pub entry_slippage_bps: Option<i32>,
    pub exit_slippage_bps: Option<i32>,
    pub mfe_bps: Option<i32>,
    pub mae_bps: Option<i32>,
    pub adv20: Option<u64>,
    pub atr14: Option<Decimal>,
    pub stop_distance_atr: Option<Decimal>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MarketDataStatus {
    Ok,
    Unavailable,
    Unsupported,
    NotApplicable,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DetailedTradeGrade {
    pub trade_id: Uuid,
    pub grade: TradeGrade,
    pub weights: GradingWeightsPermille,
    pub points: WeightedPointsBreakdown,
    pub market: MarketDataDetails,
    pub checks: Vec<String>,
}

pub struct TradeGradeService<'a> {
    database: &'a mut dyn DatabaseFactory,
    broker: &'a mut dyn Broker,
}

impl<'a> TradeGradeService<'a> {
    pub fn new(database: &'a mut dyn DatabaseFactory, broker: &'a mut dyn Broker) -> Self {
        Self { database, broker }
    }

    pub fn compute_grade(
        &mut self,
        trade_id: Uuid,
        weights: GradingWeightsPermille,
    ) -> Result<DetailedTradeGrade, Box<dyn Error>> {
        weights.validate()?;

        let trade = self.database.trade_read().read_trade(trade_id)?;
        let account = self.database.account_read().id(trade.account_id)?;
        let now = Utc::now().naive_utc();

        let (entry_fill, entry_time) = best_effort_fill(&trade.entry, trade.entry.unit_price);
        let (exit_fill, exit_time) = best_effort_exit_fill(
            &trade,
            trade.target.unit_price,
            trade.safety_stop.unit_price,
        );

        let mut checks: Vec<String> = Vec::new();
        if trade.status != Status::ClosedTarget
            && trade.status != Status::ClosedStopLoss
            && !(trade.status == Status::Canceled && exit_fill.is_some())
        {
            return Err(format!(
                "Trade {trade_id} is not closed (status={}); cannot grade",
                trade.status
            )
            .into());
        }

        if trade
            .thesis
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .is_none()
        {
            checks.push("missing:thesis".to_string());
        }

        // --- Component scores ---
        let (documentation_score, mut doc_recs) = score_documentation(&trade);
        let (process_score, mut proc_recs) = score_process(&trade);
        let (risk_score, mut risk_recs) = score_risk(self.database, &trade, entry_fill, entry_time);

        let (execution_score, mut exec_recs, market) = score_execution_with_market_data(
            self.broker,
            &account,
            &trade,
            entry_fill,
            entry_time,
            exit_fill,
            exit_time,
        );

        // --- Overall score (integer math, deterministic rounding) ---
        let overall_score = weighted_score_u8(
            process_score,
            risk_score,
            execution_score,
            documentation_score,
            weights,
        );
        let overall_grade = Grade::from_score(overall_score);

        let mut recommendations: Vec<String> = Vec::new();
        recommendations.append(&mut proc_recs);
        recommendations.append(&mut risk_recs);
        recommendations.append(&mut exec_recs);
        recommendations.append(&mut doc_recs);
        recommendations.sort();
        recommendations.dedup();

        let draft_grade = TradeGrade {
            id: Uuid::new_v4(),
            created_at: now,
            updated_at: now,
            deleted_at: None,
            trade_id: trade.id,
            overall_score,
            overall_grade,
            process_score,
            risk_score,
            execution_score,
            documentation_score,
            recommendations,
            graded_at: now,
            process_weight_permille: weights.process,
            risk_weight_permille: weights.risk,
            execution_weight_permille: weights.execution,
            documentation_weight_permille: weights.documentation,
        };

        let points = compute_points(&draft_grade, weights);
        // Keep an internal math check for CLI / agents.
        checks.push(format!(
            "math:total_points={}",
            points.total_points.round_dp(4).normalize()
        ));
        checks.push(format!("math:overall_score={}", draft_grade.overall_score));

        Ok(DetailedTradeGrade {
            trade_id: trade.id,
            grade: draft_grade,
            weights,
            points,
            market,
            checks,
        })
    }

    pub fn grade_trade(
        &mut self,
        trade_id: Uuid,
        weights: GradingWeightsPermille,
    ) -> Result<DetailedTradeGrade, Box<dyn Error>> {
        let mut computed = self.compute_grade(trade_id, weights)?;
        let persisted = self
            .database
            .trade_grade_write()
            .create_trade_grade(&computed.grade)?;
        computed.grade = persisted;
        Ok(computed)
    }

    pub fn latest_grade_for_trade(
        &mut self,
        trade_id: Uuid,
    ) -> Result<Option<TradeGrade>, Box<dyn Error>> {
        self.database
            .trade_grade_read()
            .read_latest_for_trade(trade_id)
    }

    pub fn grades_for_account_days(
        &mut self,
        account_id: Uuid,
        days: u32,
    ) -> Result<Vec<TradeGrade>, Box<dyn Error>> {
        self.database
            .trade_grade_read()
            .read_for_account_days(account_id, days)
    }
}

fn score_documentation(trade: &Trade) -> (u8, Vec<String>) {
    let mut score: i32 = 0;
    let mut recs: Vec<String> = Vec::new();

    if trade
        .thesis
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .is_some()
    {
        score = score.saturating_add(40);
    } else {
        recs.push("Add a trade thesis (why this trade exists)".to_string());
    }

    if trade
        .context
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .is_some()
    {
        score = score.saturating_add(30);
    } else {
        recs.push("Add trade context (setup, signals, levels)".to_string());
    }

    if trade
        .sector
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .is_some()
    {
        score = score.saturating_add(15);
    } else {
        recs.push("Set trade sector (for later analysis)".to_string());
    }

    if trade
        .asset_class
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .is_some()
    {
        score = score.saturating_add(15);
    } else {
        recs.push("Set trade asset_class (for later analysis)".to_string());
    }

    (clamp_score(score), recs)
}

fn score_process(trade: &Trade) -> (u8, Vec<String>) {
    let mut score: i32 = 100;
    let mut recs: Vec<String> = Vec::new();

    // Planned bracket order shape (entry=limit, stop=stop, target=limit) is the default Trust flow.
    if trade.entry.category != OrderCategory::Limit {
        score = score.saturating_sub(10);
        recs.push("Use limit orders for entries (reduce slippage)".to_string());
    }
    if trade.target.category != OrderCategory::Limit {
        score = score.saturating_sub(10);
        recs.push("Use limit orders for targets when possible".to_string());
    }
    if trade.safety_stop.category != OrderCategory::Stop {
        score = score.saturating_sub(10);
        recs.push("Use stop orders for safety stops".to_string());
    }

    // Planned risk/reward (wiki guidance: avoid 1:1, prefer >= 2:1).
    let rr = planned_rr_ratio(trade);
    if let Some(rr) = rr {
        if rr < dec!(1.0) {
            score = score.saturating_sub(40);
            recs.push("Planned R:R is < 1.0 (rework entry/stop/target)".to_string());
        } else if rr < dec!(1.5) {
            score = score.saturating_sub(25);
            recs.push(
                "Planned R:R is < 1.5 (consider improving target or tightening stop)".to_string(),
            );
        } else if rr < dec!(2.0) {
            score = score.saturating_sub(10);
            recs.push("Planned R:R is < 2.0 (aim for >= 2.0 when possible)".to_string());
        }
    } else {
        score = score.saturating_sub(20);
        recs.push("Planned R:R could not be computed (check entry/stop/target prices)".to_string());
    }

    (clamp_score(score), recs)
}

fn score_risk(
    database: &mut dyn DatabaseFactory,
    trade: &Trade,
    entry_fill: Option<Decimal>,
    entry_time: Option<NaiveDateTime>,
) -> (u8, Vec<String>) {
    let mut score: i32 = 100;
    let mut recs: Vec<String> = Vec::new();

    // 1) Stop submitted before entry fill (best-effort based on timestamps).
    if let (Some(stop_submitted), Some(entry_filled)) =
        (trade.safety_stop.submitted_at, trade.entry.filled_at)
    {
        if stop_submitted > entry_filled {
            score = score.saturating_sub(30);
            recs.push(
                "Stop order was submitted after entry filled (submit stop before entry execution)"
                    .to_string(),
            );
        }
    } else if entry_time.is_some() {
        // Entry has timing but stop doesn't.
        score = score.saturating_sub(10);
        recs.push(
            "Stop submission timestamp missing (ensure bracket orders are submitted)".to_string(),
        );
    }

    // 2) Risk per trade vs account equity and configured rules.
    let equity = database
        .account_balance_read()
        .for_currency(trade.account_id, &trade.currency)
        .map(|b| b.total_balance)
        .unwrap_or(dec!(0));

    if equity > dec!(0) {
        if let Some(entry_fill) = entry_fill {
            let risk_amount = planned_risk_amount(trade, entry_fill);
            if let Some(risk_amount) = risk_amount {
                let risk_pct = risk_amount
                    .checked_mul(dec!(100))
                    .and_then(|v| v.checked_div(equity))
                    .unwrap_or(dec!(0));

                // Basic wiki guidance: 2% rule.
                if risk_pct > dec!(2.0) {
                    score = score.saturating_sub(20);
                    recs.push(
                        "Planned risk exceeds 2% of equity (consider smaller size or tighter stop)"
                            .to_string(),
                    );
                }

                // If a rule exists, score against it.
                let rules = database
                    .rule_read()
                    .read_all_rules(trade.account_id)
                    .unwrap_or_else(|_| Vec::new());

                for rule in rules {
                    match rule.name {
                        RuleName::RiskPerTrade(limit_pct) => {
                            // Compare without float arithmetic by converting once.
                            let limit = Decimal::from_f32_retain(limit_pct).unwrap_or(dec!(0));
                            if limit > dec!(0) && risk_pct > limit {
                                score = score.saturating_sub(25);
                                recs.push(format!(
                                    "Planned risk {risk_pct}% exceeds account risk_per_trade rule ({limit}%)"
                                ));
                            }
                        }
                        RuleName::RiskPerMonth(_) => {}
                    }
                }
            }
        }
    } else {
        score = score.saturating_sub(10);
        recs.push("Account equity unavailable for risk checks".to_string());
    }

    (clamp_score(score), recs)
}

fn score_execution_with_market_data(
    broker: &mut dyn Broker,
    account: &Account,
    trade: &Trade,
    entry_fill: Option<Decimal>,
    entry_time: Option<NaiveDateTime>,
    exit_fill: Option<Decimal>,
    exit_time: Option<NaiveDateTime>,
) -> (u8, Vec<String>, MarketDataDetails) {
    let mut score: i32 = 100;
    let mut recs: Vec<String> = Vec::new();

    let entry_slip = slippage_bps(entry_fill, trade.entry.unit_price);
    let exit_intended = intended_exit_price(trade);
    let exit_slip = slippage_bps(exit_fill, exit_intended.unwrap_or(dec!(0)));

    if let Some(bps) = entry_slip {
        if bps > 50 {
            score = score.saturating_sub(10);
            recs.push(
                "Entry slippage > 0.50% (consider limit orders / more liquidity)".to_string(),
            );
        } else if bps > 10 {
            score = score.saturating_sub(5);
            recs.push("Entry slippage > 0.10% (review execution)".to_string());
        }
    } else {
        score = score.saturating_sub(10);
        recs.push("Entry fill data missing (cannot compute slippage)".to_string());
    }

    if let Some(bps) = exit_slip {
        if bps > 80 {
            score = score.saturating_sub(15);
            recs.push("Exit slippage > 0.80% (review order timing/placement)".to_string());
        } else if bps > 20 {
            score = score.saturating_sub(7);
            recs.push("Exit slippage > 0.20% (review execution)".to_string());
        }
    } else {
        score = score.saturating_sub(10);
        recs.push("Exit fill data missing (cannot compute slippage)".to_string());
    }

    // Market data derived metrics (MFE/MAE, ADV, ATR) are best-effort.
    let (market_status, timeframe, mfe_bps, mae_bps, adv20, atr14, stop_atr) =
        fetch_and_compute_market_metrics(
            broker, account, trade, entry_fill, entry_time, exit_fill, exit_time,
        );

    if let Some(adv) = adv20 {
        if adv < 500_000 {
            score = score.saturating_sub(10);
            recs.push("Low average daily volume (ADV20 < 500k); expect worse slippage".to_string());
        }
    }

    if let Some(stop_atr) = stop_atr {
        if stop_atr < dec!(1.0) {
            score = score.saturating_sub(10);
            recs.push("Stop distance < 1 ATR (may be inside normal noise)".to_string());
        }
    }

    (
        clamp_score(score),
        recs,
        MarketDataDetails {
            status: market_status,
            timeframe,
            entry_slippage_bps: entry_slip,
            exit_slippage_bps: exit_slip,
            mfe_bps,
            mae_bps,
            adv20,
            atr14,
            stop_distance_atr: stop_atr,
        },
    )
}

fn fetch_and_compute_market_metrics(
    broker: &mut dyn Broker,
    account: &Account,
    trade: &Trade,
    entry_fill: Option<Decimal>,
    entry_time: Option<NaiveDateTime>,
    exit_fill: Option<Decimal>,
    exit_time: Option<NaiveDateTime>,
) -> (
    MarketDataStatus,
    Option<BarTimeframe>,
    Option<i32>,
    Option<i32>,
    Option<u64>,
    Option<Decimal>,
    Option<Decimal>,
) {
    let symbol = trade.trading_vehicle.symbol.as_str();

    // ATR/ADV window: 30 trading days before entry (daily bars).
    let (atr14, adv20) = if let Some(entry_time) = entry_time {
        let start = entry_time.checked_sub_signed(Duration::days(40));
        let end = Some(entry_time);
        if let (Some(start), Some(end)) = (start, end) {
            let start_dt = Utc.from_utc_datetime(&start);
            let end_dt = Utc.from_utc_datetime(&end);
            match broker.get_bars(symbol, start_dt, end_dt, BarTimeframe::OneDay, account) {
                Ok(bars) => {
                    let atr = atr14_from_bars(&bars);
                    let adv = adv20_from_bars(&bars);
                    (atr, adv)
                }
                Err(_) => (None, None),
            }
        } else {
            (None, None)
        }
    } else {
        (None, None)
    };

    // Trade window: between entry and exit.
    let (mfe_bps, mae_bps, timeframe, status) = match (entry_time, exit_time, entry_fill, exit_fill)
    {
        (Some(start), Some(end), Some(entry), Some(_exit)) if end > start => {
            let tf = timeframe_for_window(start, end);
            let start_dt = Utc.from_utc_datetime(&start);
            let end_dt = Utc.from_utc_datetime(&end);
            match broker.get_bars(symbol, start_dt, end_dt, tf, account) {
                Ok(bars) => {
                    let (mfe, mae) = mfe_mae_bps(trade, entry, &bars);
                    (mfe, mae, Some(tf), MarketDataStatus::Ok)
                }
                Err(e) => {
                    let msg = format!("{e}");
                    if msg.to_lowercase().contains("unsupported") {
                        (None, None, Some(tf), MarketDataStatus::Unsupported)
                    } else {
                        (None, None, Some(tf), MarketDataStatus::Unavailable)
                    }
                }
            }
        }
        _ => (None, None, None, MarketDataStatus::NotApplicable),
    };

    let stop_atr = match (entry_fill, atr14) {
        (Some(entry), Some(atr)) if atr > dec!(0) => {
            planned_stop_distance(trade, entry).and_then(|d| d.checked_div(atr))
        }
        _ => None,
    };

    (status, timeframe, mfe_bps, mae_bps, adv20, atr14, stop_atr)
}

fn timeframe_for_window(start: NaiveDateTime, end: NaiveDateTime) -> BarTimeframe {
    let duration = end - start;
    if duration <= Duration::days(2) {
        BarTimeframe::OneMinute
    } else if duration <= Duration::days(14) {
        BarTimeframe::OneHour
    } else {
        BarTimeframe::OneDay
    }
}

fn planned_rr_ratio(trade: &Trade) -> Option<Decimal> {
    let risk = planned_stop_distance(trade, trade.entry.unit_price)?;
    let reward = planned_reward_distance(trade, trade.entry.unit_price)?;
    if risk <= dec!(0) {
        return None;
    }
    reward.checked_div(risk)
}

fn planned_risk_amount(trade: &Trade, entry_fill: Decimal) -> Option<Decimal> {
    let per_share = planned_stop_distance(trade, entry_fill)?;
    let qty = Decimal::from(trade.entry.quantity);
    per_share.checked_mul(qty)
}

fn planned_stop_distance(trade: &Trade, entry: Decimal) -> Option<Decimal> {
    match trade.category {
        model::TradeCategory::Long => entry.checked_sub(trade.safety_stop.unit_price),
        model::TradeCategory::Short => trade.safety_stop.unit_price.checked_sub(entry),
    }
}

fn planned_reward_distance(trade: &Trade, entry: Decimal) -> Option<Decimal> {
    match trade.category {
        model::TradeCategory::Long => trade.target.unit_price.checked_sub(entry),
        model::TradeCategory::Short => entry.checked_sub(trade.target.unit_price),
    }
}

fn best_effort_fill(
    order: &model::Order,
    fallback_price: Decimal,
) -> (Option<Decimal>, Option<NaiveDateTime>) {
    let price = order.average_filled_price.or(Some(fallback_price));
    (price, order.filled_at)
}

fn best_effort_exit_fill(
    trade: &Trade,
    fallback_target: Decimal,
    fallback_stop: Decimal,
) -> (Option<Decimal>, Option<NaiveDateTime>) {
    match trade.status {
        Status::ClosedTarget => (
            trade.target.average_filled_price.or(Some(fallback_target)),
            trade.target.filled_at,
        ),
        Status::ClosedStopLoss => (
            trade
                .safety_stop
                .average_filled_price
                .or(Some(fallback_stop)),
            trade.safety_stop.filled_at,
        ),
        // Canceled trades should only be considered closed if there is a real exit fill.
        // Do not synthesize a fallback fill here.
        Status::Canceled => (trade.target.average_filled_price, trade.target.filled_at),
        _ => (None, None),
    }
}

fn intended_exit_price(trade: &Trade) -> Option<Decimal> {
    match trade.status {
        Status::ClosedTarget | Status::Canceled => Some(trade.target.unit_price),
        Status::ClosedStopLoss => Some(trade.safety_stop.unit_price),
        _ => None,
    }
}

fn slippage_bps(fill: Option<Decimal>, intended: Decimal) -> Option<i32> {
    if intended <= dec!(0) {
        return None;
    }
    let fill = fill?;
    let diff = fill.checked_sub(intended)?.abs();
    let bps = diff.checked_mul(dec!(10000))?.checked_div(intended)?;
    decimal_to_i32_rounded(bps)
}

fn mfe_mae_bps(trade: &Trade, entry: Decimal, bars: &[MarketBar]) -> (Option<i32>, Option<i32>) {
    if bars.is_empty() || entry <= dec!(0) {
        return (None, None);
    }

    let mut max_high = bars[0].high;
    let mut min_low = bars[0].low;
    for bar in bars {
        if bar.high > max_high {
            max_high = bar.high;
        }
        if bar.low < min_low {
            min_low = bar.low;
        }
    }

    match trade.category {
        model::TradeCategory::Long => {
            let mfe = max_high
                .checked_sub(entry)
                .and_then(|d| d.checked_mul(dec!(10000)))
                .and_then(|d| d.checked_div(entry));
            let mae = entry
                .checked_sub(min_low)
                .and_then(|d| d.checked_mul(dec!(10000)))
                .and_then(|d| d.checked_div(entry));
            (
                mfe.and_then(decimal_to_i32_rounded),
                mae.and_then(decimal_to_i32_rounded),
            )
        }
        model::TradeCategory::Short => {
            let mfe = entry
                .checked_sub(min_low)
                .and_then(|d| d.checked_mul(dec!(10000)))
                .and_then(|d| d.checked_div(entry));
            let mae = max_high
                .checked_sub(entry)
                .and_then(|d| d.checked_mul(dec!(10000)))
                .and_then(|d| d.checked_div(entry));
            (
                mfe.and_then(decimal_to_i32_rounded),
                mae.and_then(decimal_to_i32_rounded),
            )
        }
    }
}

fn atr14_from_bars(bars: &[MarketBar]) -> Option<Decimal> {
    if bars.len() < 15 {
        return None;
    }

    // True range needs previous close.
    let mut trs: Vec<Decimal> = Vec::new();
    for i in 1..bars.len() {
        let high = bars[i].high;
        let low = bars[i].low;
        let prev_close = bars[i - 1].close;

        let tr1 = high.checked_sub(low)?;
        let tr2 = high.checked_sub(prev_close)?.abs();
        let tr3 = low.checked_sub(prev_close)?.abs();

        let tr = tr1.max(tr2).max(tr3);
        trs.push(tr);
    }

    // Last 14 TRs.
    let window = trs.len().min(14);
    if window < 14 {
        return None;
    }
    let start = trs.len() - 14;
    let slice = &trs[start..];
    let sum = slice
        .iter()
        .copied()
        .try_fold(dec!(0), |acc, v| acc.checked_add(v))?;
    sum.checked_div(Decimal::from(14))
}

fn adv20_from_bars(bars: &[MarketBar]) -> Option<u64> {
    if bars.len() < 20 {
        return None;
    }
    let start = bars.len().checked_sub(20)?;
    let slice = &bars[start..];
    let sum: u128 = slice.iter().map(|b| u128::from(b.volume)).sum();
    let avg = sum.checked_div(20u128)?;
    u64::try_from(avg).ok()
}

fn weighted_score_u8(
    process: u8,
    risk: u8,
    execution: u8,
    documentation: u8,
    weights: GradingWeightsPermille,
) -> u8 {
    let p = i32::from(process).checked_mul(i32::from(weights.process));
    let r = i32::from(risk).checked_mul(i32::from(weights.risk));
    let e = i32::from(execution).checked_mul(i32::from(weights.execution));
    let d = i32::from(documentation).checked_mul(i32::from(weights.documentation));

    let sum = p
        .and_then(|v| r.and_then(|rv| v.checked_add(rv)))
        .and_then(|v| e.and_then(|ev| v.checked_add(ev)))
        .and_then(|v| d.and_then(|dv| v.checked_add(dv)))
        .unwrap_or(0);

    // sum is in score*permille, divide by 1000 with half-up rounding.
    let rounded = sum
        .checked_add(500)
        .and_then(|v| v.checked_div(1000))
        .unwrap_or(0);

    clamp_score(rounded)
}

fn compute_points(grade: &TradeGrade, weights: GradingWeightsPermille) -> WeightedPointsBreakdown {
    let p = points_for(grade.process_score, weights.process);
    let r = points_for(grade.risk_score, weights.risk);
    let e = points_for(grade.execution_score, weights.execution);
    let d = points_for(grade.documentation_score, weights.documentation);
    let total = p
        .checked_add(r)
        .and_then(|v| v.checked_add(e))
        .and_then(|v| v.checked_add(d))
        .unwrap_or(dec!(0));
    WeightedPointsBreakdown {
        process_points: p,
        risk_points: r,
        execution_points: e,
        documentation_points: d,
        total_points: total,
    }
}

fn points_for(score: u8, weight_permille: u16) -> Decimal {
    Decimal::from(score)
        .checked_mul(Decimal::from(u32::from(weight_permille)))
        .and_then(|v| v.checked_div(Decimal::from(1000u32)))
        .unwrap_or(dec!(0))
}

fn clamp_score(value: impl Into<i32>) -> u8 {
    let v: i32 = value.into();
    v.clamp(0, 100) as u8
}

fn decimal_to_i32_rounded(value: Decimal) -> Option<i32> {
    // value is expected to be "small" (bps-like). Round half-up.
    let scaled = value.round_dp(0);
    scaled.to_i32()
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{DateTime, TimeZone};
    use model::{
        BrokerKind, BrokerLog, Currency, DraftTrade, Environment, MarketBar, OrderAction, OrderIds,
        OrderStatus, TradeCategory, TradingVehicleCategory,
    };

    #[derive(Clone)]
    enum BarsResponse {
        Bars(Vec<MarketBar>),
        Error(&'static str),
    }

    struct MarketDataBroker {
        daily: BarsResponse,
        window: BarsResponse,
    }

    impl MarketDataBroker {
        fn empty() -> Self {
            Self {
                daily: BarsResponse::Error("market data unavailable"),
                window: BarsResponse::Error("market data unavailable"),
            }
        }

        fn with_bars(daily: Vec<MarketBar>, window: Vec<MarketBar>) -> Self {
            Self {
                daily: BarsResponse::Bars(daily),
                window: BarsResponse::Bars(window),
            }
        }
    }

    impl Broker for MarketDataBroker {
        fn kind(&self) -> BrokerKind {
            BrokerKind::Alpaca
        }

        fn submit_trade(
            &self,
            _trade: &Trade,
            _account: &Account,
        ) -> Result<(BrokerLog, OrderIds), Box<dyn Error>> {
            Err("not used in grading tests".into())
        }

        fn sync_trade(
            &self,
            _trade: &Trade,
            _account: &Account,
        ) -> Result<(Status, Vec<model::Order>, BrokerLog), Box<dyn Error>> {
            Err("not used in grading tests".into())
        }

        fn close_trade(
            &self,
            _trade: &Trade,
            _account: &Account,
        ) -> Result<(model::Order, BrokerLog), Box<dyn Error>> {
            Err("not used in grading tests".into())
        }

        fn cancel_trade(&self, _trade: &Trade, _account: &Account) -> Result<(), Box<dyn Error>> {
            Err("not used in grading tests".into())
        }

        fn modify_stop(
            &self,
            _trade: &Trade,
            _account: &Account,
            _new_stop_price: Decimal,
        ) -> Result<String, Box<dyn Error>> {
            Err("not used in grading tests".into())
        }

        fn modify_target(
            &self,
            _trade: &Trade,
            _account: &Account,
            _new_price: Decimal,
        ) -> Result<String, Box<dyn Error>> {
            Err("not used in grading tests".into())
        }

        fn get_bars(
            &self,
            _symbol: &str,
            _start: DateTime<Utc>,
            _end: DateTime<Utc>,
            timeframe: BarTimeframe,
            _account: &Account,
        ) -> Result<Vec<MarketBar>, Box<dyn Error>> {
            let response = if timeframe == BarTimeframe::OneDay {
                &self.daily
            } else {
                &self.window
            };

            match response {
                BarsResponse::Bars(bars) => Ok(bars.clone()),
                BarsResponse::Error(message) => Err((*message).into()),
            }
        }
    }

    fn order(price: Decimal, category: OrderCategory, quantity: u64) -> model::Order {
        model::Order {
            unit_price: price,
            category,
            quantity,
            ..Default::default()
        }
    }

    fn trade_with_plan(
        category: TradeCategory,
        entry: Decimal,
        stop: Decimal,
        target: Decimal,
    ) -> Trade {
        Trade {
            category,
            entry: order(entry, OrderCategory::Limit, 10),
            safety_stop: order(stop, OrderCategory::Stop, 10),
            target: order(target, OrderCategory::Limit, 10),
            thesis: Some("Breakout after consolidation".to_string()),
            context: Some("Daily range expansion".to_string()),
            sector: Some("Technology".to_string()),
            asset_class: Some("Stock".to_string()),
            ..Default::default()
        }
    }

    fn market_bar(day: u32, high: Decimal, low: Decimal, close: Decimal, volume: u64) -> MarketBar {
        MarketBar {
            time: Utc.with_ymd_and_hms(2024, 1, day, 0, 0, 0).unwrap(),
            open: close,
            high,
            low,
            close,
            volume,
        }
    }

    fn persist_closed_target_trade(database: &mut db_sqlite::SqliteDatabase) -> (Account, Trade) {
        let account = database
            .account_write()
            .create(
                "grade-service-account",
                "service grade tests",
                Environment::Paper,
                dec!(20),
                dec!(10),
            )
            .expect("account should be created");
        let balance = database
            .account_balance_write()
            .create(&account, &Currency::USD)
            .expect("balance should be created");
        database
            .account_balance_write()
            .update(&balance, dec!(50000), dec!(0), dec!(50000), dec!(0))
            .expect("balance should be funded");

        let vehicle = database
            .trading_vehicle_write()
            .create_trading_vehicle(
                "AAPL",
                Some("US0378331005"),
                &TradingVehicleCategory::Stock,
                "alpaca",
            )
            .expect("vehicle should be created");
        let stop = database
            .order_write()
            .create(
                &vehicle,
                10,
                dec!(95),
                &Currency::USD,
                &OrderAction::Sell,
                &OrderCategory::Stop,
            )
            .expect("stop should be created");
        let entry = database
            .order_write()
            .create(
                &vehicle,
                10,
                dec!(100),
                &Currency::USD,
                &OrderAction::Buy,
                &OrderCategory::Limit,
            )
            .expect("entry should be created");
        let target = database
            .order_write()
            .create(
                &vehicle,
                10,
                dec!(115),
                &Currency::USD,
                &OrderAction::Sell,
                &OrderCategory::Limit,
            )
            .expect("target should be created");
        let trade = database
            .trade_write()
            .create_trade(
                DraftTrade {
                    account: account.clone(),
                    trading_vehicle: vehicle,
                    quantity: 10,
                    currency: Currency::USD,
                    category: TradeCategory::Long,
                    thesis: Some("Breakout after consolidation".to_string()),
                    sector: Some("Technology".to_string()),
                    asset_class: Some("Stock".to_string()),
                    context: Some("Daily range expansion".to_string()),
                },
                &stop,
                &entry,
                &target,
            )
            .expect("trade should be created");

        let entry_time = Utc
            .with_ymd_and_hms(2024, 1, 31, 9, 30, 0)
            .unwrap()
            .naive_utc();
        let exit_time = Utc
            .with_ymd_and_hms(2024, 1, 31, 10, 30, 0)
            .unwrap()
            .naive_utc();
        let mut entry = trade.entry.clone();
        entry.average_filled_price = Some(entry.unit_price);
        entry.filled_quantity = entry.quantity;
        entry.status = OrderStatus::Filled;
        entry.filled_at = Some(entry_time);
        database
            .order_write()
            .update(&entry)
            .expect("entry fill should be persisted");

        let mut target = trade.target.clone();
        target.average_filled_price = Some(target.unit_price);
        target.filled_quantity = target.quantity;
        target.status = OrderStatus::Filled;
        target.filled_at = Some(exit_time);
        database
            .order_write()
            .update(&target)
            .expect("target fill should be persisted");

        let trade = database
            .trade_read()
            .read_trade(trade.id)
            .expect("trade should be readable");
        let trade = database
            .trade_write()
            .update_trade_status(Status::ClosedTarget, &trade)
            .expect("trade should be closed");

        (account, trade)
    }

    #[test]
    fn test_weighted_score_math_is_deterministic_and_sums() {
        let weights = GradingWeightsPermille::default();
        weights.validate().unwrap();

        let score = weighted_score_u8(90, 95, 80, 75, weights);
        assert_eq!(score, 88); // (90*0.4)+(95*0.3)+(80*0.2)+(75*0.1)=88.0
    }

    #[test]
    fn grading_weights_validate_rejects_non_1000_sum() {
        let weights = GradingWeightsPermille {
            process: 400,
            risk: 300,
            execution: 200,
            documentation: 99,
        };

        let err = weights.validate().expect_err("invalid sum should fail");

        assert!(err
            .to_string()
            .contains("expected sum=1000 permille, got 999"));
    }

    #[test]
    fn test_slippage_bps_rounding() {
        // 0.5% = 50 bps
        let fill = Some(dec!(100.50));
        let intended = dec!(100);
        assert_eq!(slippage_bps(fill, intended), Some(50));
    }

    #[test]
    fn slippage_bps_uses_absolute_difference_and_rejects_non_positive_intended() {
        assert_eq!(slippage_bps(Some(dec!(99)), dec!(100)), Some(100));
        assert_eq!(slippage_bps(Some(dec!(100)), dec!(0)), None);
        assert_eq!(slippage_bps(None, dec!(100)), None);
    }

    #[test]
    fn score_documentation_rewards_complete_metadata_and_lists_missing_fields() {
        let complete = trade_with_plan(TradeCategory::Long, dec!(100), dec!(95), dec!(115));
        let (score, recs) = score_documentation(&complete);
        assert_eq!(score, 100);
        assert!(recs.is_empty());

        let missing = Trade::default();
        let (score, recs) = score_documentation(&missing);
        assert_eq!(score, 0);
        assert_eq!(recs.len(), 4);
        assert!(recs.iter().any(|rec| rec.contains("trade thesis")));
        assert!(recs.iter().any(|rec| rec.contains("trade context")));
        assert!(recs.iter().any(|rec| rec.contains("sector")));
        assert!(recs.iter().any(|rec| rec.contains("asset_class")));
    }

    #[test]
    fn score_process_penalizes_bad_order_shape_and_low_reward_to_risk() {
        let trade = Trade {
            entry: order(dec!(100), OrderCategory::Market, 10),
            safety_stop: order(dec!(95), OrderCategory::Limit, 10),
            target: order(dec!(104), OrderCategory::Market, 10),
            ..trade_with_plan(TradeCategory::Long, dec!(100), dec!(95), dec!(104))
        };

        let (score, recs) = score_process(&trade);

        assert_eq!(score, 30);
        assert!(recs
            .iter()
            .any(|rec| rec.contains("limit orders for entries")));
        assert!(recs
            .iter()
            .any(|rec| rec.contains("limit orders for targets")));
        assert!(recs
            .iter()
            .any(|rec| rec.contains("stop orders for safety stops")));
        assert!(recs.iter().any(|rec| rec.contains("Planned R:R is < 1.0")));
    }

    #[test]
    fn score_process_penalizes_uncomputable_reward_to_risk() {
        let trade = trade_with_plan(TradeCategory::Long, dec!(100), dec!(100), dec!(115));

        let (score, recs) = score_process(&trade);

        assert_eq!(score, 80);
        assert!(recs.iter().any(|rec| rec.contains("could not be computed")));
    }

    #[test]
    fn market_data_broker_stub_non_market_methods_fail_fast() {
        let broker = MarketDataBroker::empty();
        let trade = Trade::default();
        let account = Account::default();

        assert_eq!(broker.kind(), BrokerKind::Alpaca);
        assert!(broker.submit_trade(&trade, &account).is_err());
        assert!(broker.sync_trade(&trade, &account).is_err());
        assert!(broker.close_trade(&trade, &account).is_err());
        assert!(broker.cancel_trade(&trade, &account).is_err());
        assert!(broker.modify_stop(&trade, &account, dec!(99)).is_err());
        assert!(broker.modify_target(&trade, &account, dec!(101)).is_err());
    }

    #[test]
    fn grade_trade_persists_and_service_read_methods_return_grade_snapshots() {
        let mut database = db_sqlite::SqliteDatabase::new_in_memory();
        let mut broker = MarketDataBroker::empty();
        let (account, trade) = persist_closed_target_trade(&mut database);
        let mut service = TradeGradeService::new(&mut database, &mut broker);

        assert!(service
            .latest_grade_for_trade(trade.id)
            .expect("empty latest grade lookup should succeed")
            .is_none());

        let detailed = service
            .grade_trade(trade.id, GradingWeightsPermille::default())
            .expect("closed trade should be graded");

        let latest = service
            .latest_grade_for_trade(trade.id)
            .expect("latest grade lookup should succeed")
            .expect("persisted grade should exist");
        assert_eq!(latest.id, detailed.grade.id);
        assert_eq!(latest.trade_id, trade.id);

        let account_grades = service
            .grades_for_account_days(account.id, 30)
            .expect("account grade lookup should succeed");
        assert_eq!(account_grades.len(), 1);
        assert_eq!(account_grades[0].id, detailed.grade.id);
        assert_eq!(detailed.trade_id, trade.id);
        assert_eq!(detailed.weights, GradingWeightsPermille::default());
    }

    #[test]
    fn planned_reward_risk_math_handles_long_short_and_invalid_geometry() {
        let long = trade_with_plan(TradeCategory::Long, dec!(100), dec!(95), dec!(115));
        assert_eq!(planned_stop_distance(&long, dec!(100)), Some(dec!(5)));
        assert_eq!(planned_reward_distance(&long, dec!(100)), Some(dec!(15)));
        assert_eq!(planned_rr_ratio(&long), Some(dec!(3)));
        assert_eq!(planned_risk_amount(&long, dec!(100)), Some(dec!(50)));

        let short = trade_with_plan(TradeCategory::Short, dec!(100), dec!(105), dec!(90));
        assert_eq!(planned_stop_distance(&short, dec!(100)), Some(dec!(5)));
        assert_eq!(planned_reward_distance(&short, dec!(100)), Some(dec!(10)));
        assert_eq!(planned_rr_ratio(&short), Some(dec!(2)));

        let invalid = trade_with_plan(TradeCategory::Long, dec!(100), dec!(100), dec!(115));
        assert_eq!(planned_rr_ratio(&invalid), None);
    }

    #[test]
    fn score_risk_penalizes_late_stop_submission_and_missing_equity() {
        let mut database = db_sqlite::SqliteDatabase::new_in_memory();
        let entry_filled = Utc
            .with_ymd_and_hms(2024, 1, 1, 9, 35, 0)
            .unwrap()
            .naive_utc();
        let stop_submitted = Utc
            .with_ymd_and_hms(2024, 1, 1, 9, 36, 0)
            .unwrap()
            .naive_utc();
        let mut trade = trade_with_plan(TradeCategory::Long, dec!(100), dec!(95), dec!(115));
        trade.entry.filled_at = Some(entry_filled);
        trade.safety_stop.submitted_at = Some(stop_submitted);

        let (score, recs) = score_risk(&mut database, &trade, Some(dec!(100)), Some(entry_filled));

        assert_eq!(score, 60);
        assert!(recs
            .iter()
            .any(|rec| rec.contains("submitted after entry filled")));
        assert!(recs.iter().any(|rec| rec.contains("equity unavailable")));
    }

    #[test]
    fn score_execution_penalizes_missing_fills_and_moderate_exit_slippage() {
        let account = Account::default();
        let trade = trade_with_plan(TradeCategory::Long, dec!(100), dec!(95), dec!(110));
        let mut broker = MarketDataBroker::empty();

        let (score, recs, market) =
            score_execution_with_market_data(&mut broker, &account, &trade, None, None, None, None);

        assert_eq!(score, 80);
        assert_eq!(market.status, MarketDataStatus::NotApplicable);
        assert!(recs.iter().any(|rec| rec.contains("Entry fill data")));
        assert!(recs.iter().any(|rec| rec.contains("Exit fill data")));

        let closed_trade = Trade {
            status: Status::ClosedTarget,
            ..trade_with_plan(TradeCategory::Long, dec!(100), dec!(95), dec!(110))
        };
        let (score, recs, market) = score_execution_with_market_data(
            &mut broker,
            &account,
            &closed_trade,
            Some(dec!(100)),
            None,
            Some(dec!(110.50)),
            None,
        );

        assert_eq!(score, 93);
        assert_eq!(market.exit_slippage_bps, Some(45));
        assert!(recs.iter().any(|rec| rec.contains("Exit slippage > 0.20%")));
    }

    #[test]
    fn score_execution_uses_market_bars_for_liquidity_volatility_and_excursion() {
        let account = Account::default();
        let entry_time = Utc
            .with_ymd_and_hms(2024, 1, 31, 9, 30, 0)
            .unwrap()
            .naive_utc();
        let exit_time = Utc
            .with_ymd_and_hms(2024, 1, 31, 10, 30, 0)
            .unwrap()
            .naive_utc();
        let mut trade = trade_with_plan(TradeCategory::Long, dec!(100), dec!(95), dec!(110));
        trade.status = Status::ClosedTarget;
        trade.entry.filled_at = Some(entry_time);
        trade.target.filled_at = Some(exit_time);
        let daily_bars = (1..=20)
            .map(|day| market_bar(day, dec!(103), dec!(93), dec!(100), 100_000))
            .collect();
        let window_bars = vec![
            market_bar(31, dec!(108), dec!(97), dec!(104), 10_000),
            market_bar(31, dec!(112), dec!(94), dec!(110), 10_000),
        ];
        let mut broker = MarketDataBroker::with_bars(daily_bars, window_bars);

        let (score, recs, market) = score_execution_with_market_data(
            &mut broker,
            &account,
            &trade,
            Some(dec!(100)),
            Some(entry_time),
            Some(dec!(110)),
            Some(exit_time),
        );

        assert_eq!(score, 80);
        assert_eq!(market.status, MarketDataStatus::Ok);
        assert_eq!(market.timeframe, Some(BarTimeframe::OneMinute));
        assert_eq!(market.mfe_bps, Some(1200));
        assert_eq!(market.mae_bps, Some(600));
        assert_eq!(market.adv20, Some(100_000));
        assert_eq!(market.atr14, Some(dec!(10)));
        assert_eq!(market.stop_distance_atr, Some(dec!(0.5)));
        assert!(recs.iter().any(|rec| rec.contains("ADV20 < 500k")));
        assert!(recs.iter().any(|rec| rec.contains("Stop distance < 1 ATR")));
    }

    #[test]
    fn fetch_market_metrics_marks_unsupported_window_errors() {
        let account = Account::default();
        let entry_time = Utc
            .with_ymd_and_hms(2024, 1, 31, 9, 30, 0)
            .unwrap()
            .naive_utc();
        let exit_time = Utc
            .with_ymd_and_hms(2024, 1, 31, 10, 30, 0)
            .unwrap()
            .naive_utc();
        let trade = trade_with_plan(TradeCategory::Long, dec!(100), dec!(95), dec!(110));
        let mut broker = MarketDataBroker {
            daily: BarsResponse::Error("daily unavailable"),
            window: BarsResponse::Error("unsupported timeframe"),
        };

        let (status, timeframe, mfe, mae, adv20, atr14, stop_atr) =
            fetch_and_compute_market_metrics(
                &mut broker,
                &account,
                &trade,
                Some(dec!(100)),
                Some(entry_time),
                Some(dec!(110)),
                Some(exit_time),
            );

        assert_eq!(status, MarketDataStatus::Unsupported);
        assert_eq!(timeframe, Some(BarTimeframe::OneMinute));
        assert_eq!(mfe, None);
        assert_eq!(mae, None);
        assert_eq!(adv20, None);
        assert_eq!(atr14, None);
        assert_eq!(stop_atr, None);
    }

    #[test]
    fn test_mfe_mae_long() {
        let mut trade = Trade::default();
        trade.category = TradeCategory::Long;

        let entry = dec!(100);
        let bars = vec![
            MarketBar {
                time: Utc.with_ymd_and_hms(2024, 1, 1, 0, 0, 0).unwrap(),
                open: dec!(100),
                high: dec!(110),
                low: dec!(95),
                close: dec!(105),
                volume: 1000,
            },
            MarketBar {
                time: Utc.with_ymd_and_hms(2024, 1, 1, 0, 1, 0).unwrap(),
                open: dec!(105),
                high: dec!(112),
                low: dec!(98),
                close: dec!(110),
                volume: 1000,
            },
        ];
        let (mfe, mae) = mfe_mae_bps(&trade, entry, &bars);
        assert_eq!(mfe, Some(1200)); // 112-100 = 12% => 1200 bps
        assert_eq!(mae, Some(500)); // 100-95 = 5% => 500 bps
    }

    #[test]
    fn mfe_mae_handles_short_trades_empty_bars_and_zero_entry() {
        let mut trade = Trade::default();
        trade.category = TradeCategory::Short;
        let bars = vec![
            market_bar(1, dec!(104), dec!(92), dec!(98), 1000),
            market_bar(2, dec!(105), dec!(90), dec!(95), 1000),
        ];

        let (mfe, mae) = mfe_mae_bps(&trade, dec!(100), &bars);
        assert_eq!(mfe, Some(1000));
        assert_eq!(mae, Some(500));
        assert_eq!(mfe_mae_bps(&trade, dec!(100), &[]), (None, None));
        assert_eq!(mfe_mae_bps(&trade, dec!(0), &bars), (None, None));
    }

    #[test]
    fn test_atr14_requires_enough_bars() {
        let bars: Vec<MarketBar> = Vec::new();
        assert_eq!(atr14_from_bars(&bars), None);
    }

    #[test]
    fn atr14_and_adv20_use_recent_complete_windows() {
        let bars: Vec<MarketBar> = (0..20)
            .map(|index| {
                let day = u32::try_from(index + 1).unwrap();
                let volume = u64::try_from(index).unwrap() + 1000;
                market_bar(day, dec!(11), dec!(9), dec!(10), volume)
            })
            .collect();

        assert_eq!(atr14_from_bars(&bars), Some(dec!(2)));
        assert_eq!(adv20_from_bars(&bars), Some(1009));
        assert_eq!(adv20_from_bars(&bars[..19]), None);
    }

    #[test]
    fn timeframe_for_window_uses_duration_boundaries() {
        let start = Utc
            .with_ymd_and_hms(2024, 1, 1, 0, 0, 0)
            .unwrap()
            .naive_utc();

        assert_eq!(
            timeframe_for_window(start, start + Duration::days(2)),
            BarTimeframe::OneMinute
        );
        assert_eq!(
            timeframe_for_window(start, start + Duration::days(14)),
            BarTimeframe::OneHour
        );
        assert_eq!(
            timeframe_for_window(start, start + Duration::days(15)),
            BarTimeframe::OneDay
        );
    }

    #[test]
    fn best_effort_fill_and_exit_fill_cover_status_specific_fallbacks() {
        let filled_at = Utc
            .with_ymd_and_hms(2024, 1, 1, 9, 30, 0)
            .unwrap()
            .naive_utc();
        let order = model::Order {
            unit_price: dec!(100),
            average_filled_price: Some(dec!(101)),
            filled_at: Some(filled_at),
            ..Default::default()
        };
        assert_eq!(
            best_effort_fill(&order, dec!(99)),
            (Some(dec!(101)), Some(filled_at))
        );

        let target_trade = Trade {
            status: Status::ClosedTarget,
            target: model::Order {
                unit_price: dec!(120),
                average_filled_price: Some(dec!(119)),
                filled_at: Some(filled_at),
                ..Default::default()
            },
            ..Default::default()
        };
        assert_eq!(
            best_effort_exit_fill(&target_trade, dec!(120), dec!(95)),
            (Some(dec!(119)), Some(filled_at))
        );

        let stop_trade = Trade {
            status: Status::ClosedStopLoss,
            safety_stop: model::Order {
                unit_price: dec!(95),
                average_filled_price: None,
                filled_at: Some(filled_at),
                ..Default::default()
            },
            ..Default::default()
        };
        assert_eq!(
            best_effort_exit_fill(&stop_trade, dec!(120), dec!(95)),
            (Some(dec!(95)), Some(filled_at))
        );

        let canceled_with_fill = Trade {
            status: Status::Canceled,
            target: model::Order {
                average_filled_price: Some(dec!(118)),
                filled_at: Some(filled_at),
                ..Default::default()
            },
            ..Default::default()
        };
        assert_eq!(
            best_effort_exit_fill(&canceled_with_fill, dec!(120), dec!(95)),
            (Some(dec!(118)), Some(filled_at))
        );

        let open_trade = Trade {
            status: Status::Filled,
            ..Default::default()
        };
        assert_eq!(
            best_effort_exit_fill(&open_trade, dec!(120), dec!(95)),
            (None, None)
        );
    }

    #[test]
    fn intended_exit_price_tracks_terminal_status() {
        let target_trade = Trade {
            status: Status::ClosedTarget,
            target: order(dec!(120), OrderCategory::Limit, 10),
            ..Default::default()
        };
        let stop_trade = Trade {
            status: Status::ClosedStopLoss,
            safety_stop: order(dec!(95), OrderCategory::Stop, 10),
            ..Default::default()
        };
        let open_trade = Trade {
            status: Status::Filled,
            ..Default::default()
        };

        assert_eq!(intended_exit_price(&target_trade), Some(dec!(120)));
        assert_eq!(intended_exit_price(&stop_trade), Some(dec!(95)));
        assert_eq!(intended_exit_price(&open_trade), None);
    }

    #[test]
    fn compute_points_breakdown_matches_weighted_scores() {
        let weights = GradingWeightsPermille {
            process: 400,
            risk: 300,
            execution: 200,
            documentation: 100,
        };
        let now = Utc::now().naive_utc();
        let grade = TradeGrade {
            id: Uuid::new_v4(),
            created_at: now,
            updated_at: now,
            deleted_at: None,
            trade_id: Uuid::new_v4(),
            overall_score: 79,
            overall_grade: Grade::CPlus,
            process_score: 80,
            risk_score: 90,
            execution_score: 70,
            documentation_score: 60,
            recommendations: Vec::new(),
            graded_at: now,
            process_weight_permille: weights.process,
            risk_weight_permille: weights.risk,
            execution_weight_permille: weights.execution,
            documentation_weight_permille: weights.documentation,
        };

        let points = compute_points(&grade, weights);

        assert_eq!(points.process_points, dec!(32));
        assert_eq!(points.risk_points, dec!(27));
        assert_eq!(points.execution_points, dec!(14));
        assert_eq!(points.documentation_points, dec!(6));
        assert_eq!(points.total_points, dec!(79));
    }

    #[test]
    fn test_canceled_trade_does_not_get_synthetic_exit_fill() {
        let mut trade = Trade::default();
        trade.status = Status::Canceled;
        trade.target.unit_price = dec!(123);
        trade.target.average_filled_price = None;
        trade.target.filled_at = None;

        let (exit_fill, exit_time) = best_effort_exit_fill(
            &trade,
            trade.target.unit_price,
            trade.safety_stop.unit_price,
        );
        assert_eq!(exit_fill, None);
        assert_eq!(exit_time, None);
    }
}
