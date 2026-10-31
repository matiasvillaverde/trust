//! Legacy report calculations command responsibility. Business mutations go through TrustFacade.
use chrono::DateTime;
use chrono::Days;
use chrono::NaiveDate;
use chrono::Utc;
use core::TrustFacade;
use model::Trade;
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use uuid::Uuid;

pub(in crate::dispatcher) fn transactions_for_scope(
    trust: &mut TrustFacade,
    account_id: Option<Uuid>,
    days_filter: Option<u32>,
) -> Vec<model::transaction::Transaction> {
    let mut transactions = if let Some(id) = account_id {
        trust
            .get_account_transactions(id)
            .unwrap_or_else(|_| Vec::new())
    } else {
        trust.get_all_transactions().unwrap_or_else(|_| Vec::new())
    };

    if let Some(days) = days_filter {
        let now = Utc::now().naive_utc();
        let cutoff = now
            .checked_sub_signed(chrono::Duration::days(i64::from(days)))
            .unwrap_or(now);
        transactions.retain(|tx| tx.created_at >= cutoff);
    }

    transactions
}

pub(in crate::dispatcher) fn drawdown_metrics_for_transactions(
    transactions: &[model::transaction::Transaction],
) -> core::calculators_drawdown::DrawdownMetrics {
    use core::calculators_drawdown::{
        DrawdownMetrics, RealizedDrawdownCalculator, RealizedEquityCurve,
    };

    let curve = RealizedDrawdownCalculator::calculate_equity_curve(transactions)
        .unwrap_or(RealizedEquityCurve { points: Vec::new() });

    RealizedDrawdownCalculator::calculate_drawdown_metrics(&curve).unwrap_or(DrawdownMetrics {
        current_equity: dec!(0),
        peak_equity: dec!(0),
        current_drawdown: dec!(0),
        current_drawdown_percentage: dec!(0),
        max_drawdown: dec!(0),
        max_drawdown_percentage: dec!(0),
        max_drawdown_date: None,
        recovery_from_max: dec!(0),
        recovery_percentage: dec!(0),
        days_since_peak: 0,
        days_in_drawdown: 0,
    })
}

pub(in crate::dispatcher) fn starting_equity_for_transactions(
    transactions: &[model::transaction::Transaction],
) -> Option<Decimal> {
    use core::calculators_drawdown::RealizedDrawdownCalculator;

    let curve = RealizedDrawdownCalculator::calculate_equity_curve(transactions).ok()?;
    curve.points.first().map(|point| point.balance)
}

pub(in crate::dispatcher) fn report_calmar_ratio(
    closed_trades: &[Trade],
    transactions: &[model::transaction::Transaction],
    drawdown_metrics: &core::calculators_drawdown::DrawdownMetrics,
) -> Option<Decimal> {
    let starting_equity = starting_equity_for_transactions(transactions)?;
    core::calculators_advanced_metrics::AdvancedMetricsCalculator::calculate_calmar_ratio_for_report(
        closed_trades,
        starting_equity,
        drawdown_metrics.max_drawdown_percentage,
    )
}

pub(in crate::dispatcher) fn open_trades_for_scope(
    trust: &mut TrustFacade,
    account_id: Option<Uuid>,
) -> Vec<Trade> {
    use std::collections::HashSet;

    let mut out: Vec<Trade> = Vec::new();
    let mut seen: HashSet<Uuid> = HashSet::new();

    let mut push_unique = |trades: Vec<Trade>| {
        for t in trades {
            if seen.insert(t.id) {
                out.push(t);
            }
        }
    };

    let statuses = [model::Status::Filled, model::Status::PartiallyFilled];

    if let Some(id) = account_id {
        for status in statuses {
            if let Ok(trades) = trust.search_trades(id, status) {
                push_unique(trades);
            }
        }
        return out;
    }

    if let Ok(accounts) = trust.search_all_accounts() {
        for account in accounts {
            for status in statuses {
                if let Ok(trades) = trust.search_trades(account.id, status) {
                    push_unique(trades);
                }
            }
        }
    }

    out
}

pub(in crate::dispatcher) fn parse_period_window(
    period: &str,
    now: DateTime<Utc>,
) -> Result<(Option<NaiveDate>, Option<NaiveDate>), String> {
    let normalized = period.trim().to_ascii_lowercase();
    if let Some(days) = normalized
        .strip_prefix("last-")
        .and_then(|s| s.strip_suffix("-days"))
        .and_then(|s| s.parse::<u64>().ok())
    {
        let end = now.date_naive();
        let start = end
            .checked_sub_days(Days::new(days.saturating_sub(1)))
            .ok_or("Invalid last-N-days window")?;
        return Ok((Some(start), Some(end)));
    }
    if let Some(days) = normalized
        .strip_prefix("previous-")
        .and_then(|s| s.strip_suffix("-days"))
        .and_then(|s| s.parse::<u64>().ok())
    {
        let end = now
            .date_naive()
            .checked_sub_days(Days::new(days))
            .ok_or("Invalid previous-N-days window")?;
        let start = end
            .checked_sub_days(Days::new(days.saturating_sub(1)))
            .ok_or("Invalid previous-N-days window")?;
        return Ok((Some(start), Some(end)));
    }
    if let Some((from, to)) = normalized.split_once("..") {
        let from = NaiveDate::parse_from_str(from, "%Y-%m-%d")
            .map_err(|_| "Invalid period start date (expected YYYY-MM-DD)")?;
        let to = NaiveDate::parse_from_str(to, "%Y-%m-%d")
            .map_err(|_| "Invalid period end date (expected YYYY-MM-DD)")?;
        if from > to {
            return Err("Invalid period range: start date is after end date".to_string());
        }
        return Ok((Some(from), Some(to)));
    }
    if normalized == "all" {
        return Ok((None, None));
    }
    Err(
        "Invalid period format. Use last-N-days, previous-N-days, YYYY-MM-DD..YYYY-MM-DD, or all"
            .to_string(),
    )
}

pub(in crate::dispatcher) fn trade_in_window(
    trade: &Trade,
    from: Option<NaiveDate>,
    to: Option<NaiveDate>,
) -> bool {
    let day = trade.updated_at.date();
    if let Some(f) = from {
        if day < f {
            return false;
        }
    }
    if let Some(t) = to {
        if day > t {
            return false;
        }
    }
    true
}

pub(in crate::dispatcher) fn max_drawdown_amount(trades: &[Trade]) -> Decimal {
    let mut running = Decimal::ZERO;
    let mut peak = Decimal::ZERO;
    let mut max_drawdown = Decimal::ZERO;

    for trade in trades {
        running = running
            .checked_add(trade.balance.total_performance)
            .unwrap_or(running);
        if running > peak {
            peak = running;
        }
        let drawdown = peak.checked_sub(running).unwrap_or(Decimal::ZERO);
        if drawdown > max_drawdown {
            max_drawdown = drawdown;
        }
    }
    max_drawdown
}
