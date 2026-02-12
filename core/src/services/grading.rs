use chrono::{Duration, NaiveDateTime, TimeZone, Utc};
use model::{
    Account, BarTimeframe, Broker, DatabaseFactory, Grade, MarketBar, OrderCategory, RuleName,
    Status, Trade, TradeGrade,
};
use rust_decimal::Decimal;
use rust_decimal::prelude::ToPrimitive;
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
            return Err(format!(
                "Invalid grading weights: expected sum=1000 permille, got {sum}"
            )
            .into());
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
        let (exit_fill, exit_time) = best_effort_exit_fill(&trade, trade.target.unit_price, trade.safety_stop.unit_price);

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
        checks.push(format!(
            "math:overall_score={}",
            draft_grade.overall_score
        ));

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
        self.database.trade_grade_read().read_latest_for_trade(trade_id)
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
            recs.push("Planned R:R is < 1.5 (consider improving target or tightening stop)".to_string());
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
    if let (Some(stop_submitted), Some(entry_filled)) = (trade.safety_stop.submitted_at, trade.entry.filled_at) {
        if stop_submitted > entry_filled {
            score = score.saturating_sub(30);
            recs.push("Stop order was submitted after entry filled (submit stop before entry execution)".to_string());
        }
    } else if entry_time.is_some() {
        // Entry has timing but stop doesn't.
        score = score.saturating_sub(10);
        recs.push("Stop submission timestamp missing (ensure bracket orders are submitted)".to_string());
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
                    recs.push("Planned risk exceeds 2% of equity (consider smaller size or tighter stop)".to_string());
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
            recs.push("Entry slippage > 0.50% (consider limit orders / more liquidity)".to_string());
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
            broker,
            account,
            trade,
            entry_fill,
            entry_time,
            exit_fill,
            exit_time,
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
            planned_stop_distance(trade, entry)
                .and_then(|d| d.checked_div(atr))
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

fn best_effort_fill(order: &model::Order, fallback_price: Decimal) -> (Option<Decimal>, Option<NaiveDateTime>) {
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
            trade.safety_stop
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
    let bps = diff
        .checked_mul(dec!(10000))?
        .checked_div(intended)?;
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
            let mfe = max_high.checked_sub(entry).and_then(|d| d.checked_mul(dec!(10000))).and_then(|d| d.checked_div(entry));
            let mae = entry.checked_sub(min_low).and_then(|d| d.checked_mul(dec!(10000))).and_then(|d| d.checked_div(entry));
            (mfe.and_then(decimal_to_i32_rounded), mae.and_then(decimal_to_i32_rounded))
        }
        model::TradeCategory::Short => {
            let mfe = entry.checked_sub(min_low).and_then(|d| d.checked_mul(dec!(10000))).and_then(|d| d.checked_div(entry));
            let mae = max_high.checked_sub(entry).and_then(|d| d.checked_mul(dec!(10000))).and_then(|d| d.checked_div(entry));
            (mfe.and_then(decimal_to_i32_rounded), mae.and_then(decimal_to_i32_rounded))
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
    let sum = slice.iter().copied().try_fold(dec!(0), |acc, v| acc.checked_add(v))?;
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
    use chrono::TimeZone;
    use model::{MarketBar, TradeCategory};

    #[test]
    fn test_weighted_score_math_is_deterministic_and_sums() {
        let weights = GradingWeightsPermille::default();
        weights.validate().unwrap();

        let score = weighted_score_u8(90, 95, 80, 75, weights);
        assert_eq!(score, 88); // (90*0.4)+(95*0.3)+(80*0.2)+(75*0.1)=88.0
    }

    #[test]
    fn test_slippage_bps_rounding() {
        // 0.5% = 50 bps
        let fill = Some(dec!(100.50));
        let intended = dec!(100);
        assert_eq!(slippage_bps(fill, intended), Some(50));
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
    fn test_atr14_requires_enough_bars() {
        let bars: Vec<MarketBar> = Vec::new();
        assert_eq!(atr14_from_bars(&bars), None);
    }

    #[test]
    fn test_canceled_trade_does_not_get_synthetic_exit_fill() {
        let mut trade = Trade::default();
        trade.status = Status::Canceled;
        trade.target.unit_price = dec!(123);
        trade.target.average_filled_price = None;
        trade.target.filled_at = None;

        let (exit_fill, exit_time) =
            best_effort_exit_fill(&trade, trade.target.unit_price, trade.safety_stop.unit_price);
        assert_eq!(exit_fill, None);
        assert_eq!(exit_time, None);
    }
}
