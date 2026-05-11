//! Realized gains tax reporting.

use crate::services::wash_sale::WashSaleReport;
use chrono::{Days, NaiveDate};
use model::{Status, Trade};
use rust_decimal::Decimal;
use std::collections::BTreeMap;
use std::error::Error;
use std::fmt::{Display, Formatter};
use uuid::Uuid;

const LONG_TERM_THRESHOLD_DAYS: u64 = 366;

/// Error returned when tax analysis cannot be computed safely.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaxReportError {
    context: &'static str,
}

impl TaxReportError {
    fn calculation(context: &'static str) -> Self {
        Self { context }
    }
}

impl Display for TaxReportError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "tax report calculation failed: {}", self.context)
    }
}

impl Error for TaxReportError {}

/// Holding-period tax bucket for a closed trade.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaxHoldingPeriod {
    /// Position held one year or less.
    ShortTerm,
    /// Position held more than one year.
    LongTerm,
}

impl TaxHoldingPeriod {
    /// Stable JSON/text representation.
    pub fn as_str(self) -> &'static str {
        match self {
            TaxHoldingPeriod::ShortTerm => "short_term",
            TaxHoldingPeriod::LongTerm => "long_term",
        }
    }
}

/// Short-term and long-term tax rates expressed as decimals between 0 and 1.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TaxRates {
    /// Rate applied to short-term taxable gains.
    pub short_term: Decimal,
    /// Rate applied to long-term taxable gains.
    pub long_term: Decimal,
}

impl TaxRates {
    /// Create tax rates after validating the 0..=1 range.
    pub fn new(short_term: Decimal, long_term: Decimal) -> Result<Self, TaxReportError> {
        validate_rate(short_term)?;
        validate_rate(long_term)?;
        Ok(Self {
            short_term,
            long_term,
        })
    }
}

/// Optional report-time rate overrides.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct TaxRateOverrides {
    /// Override short-term rates for every account in the report.
    pub short_term: Option<Decimal>,
    /// Override long-term rates for every account in the report.
    pub long_term: Option<Decimal>,
}

/// Per-trade tax line.
#[derive(Debug, Clone, PartialEq)]
pub struct TaxTradeLine {
    /// Account that owns the trade.
    pub account_id: Uuid,
    /// Trade identifier.
    pub trade_id: Uuid,
    /// Normalized symbol.
    pub symbol: String,
    /// Entry fill date.
    pub opened_date: NaiveDate,
    /// Exit fill date.
    pub closed_date: NaiveDate,
    /// Number of calendar days between entry and exit fills.
    pub holding_days: i64,
    /// Short-term or long-term bucket.
    pub holding_period: TaxHoldingPeriod,
    /// Persisted realized P&L before tax adjustments.
    pub realized_gain_loss: Decimal,
    /// Wash-sale loss disallowed on this trade.
    pub wash_sale_disallowed_loss: Decimal,
    /// Replacement basis adjustment applied when this trade is a closed replacement.
    pub replacement_basis_adjustment: Decimal,
    /// Realized P&L after wash-sale and replacement-basis adjustments.
    pub taxable_gain_loss: Decimal,
    /// Tax rate applied to this line's holding-period bucket.
    pub tax_rate: Decimal,
    /// Line-level estimated liability for positive taxable gains.
    pub estimated_tax_liability: Decimal,
}

/// Aggregated account tax summary.
#[derive(Debug, Clone, PartialEq)]
pub struct TaxAccountSummary {
    /// Account identifier.
    pub account_id: Uuid,
    /// Short-term tax rate used for this account.
    pub short_term_rate: Decimal,
    /// Long-term tax rate used for this account.
    pub long_term_rate: Decimal,
    /// Number of classified closed trades.
    pub trade_count: usize,
    /// Gross realized gains before adjustments.
    pub gross_realized_gain: Decimal,
    /// Gross realized losses before adjustments.
    pub gross_realized_loss: Decimal,
    /// Wash-sale losses disallowed for this account.
    pub wash_sale_disallowed_loss: Decimal,
    /// Replacement basis adjustments applied to closed replacement trades.
    pub replacement_basis_adjustment: Decimal,
    /// Net taxable short-term gain/loss.
    pub short_term_taxable_gain: Decimal,
    /// Net taxable long-term gain/loss.
    pub long_term_taxable_gain: Decimal,
    /// Net taxable gain/loss across holding-period buckets.
    pub net_taxable_gain: Decimal,
    /// Sum of positive line-level tax estimates before category loss netting.
    pub estimated_gross_tax_liability: Decimal,
    /// Estimated liability after netting losses inside each holding-period bucket.
    pub estimated_net_tax_liability: Decimal,
}

/// Realized gains tax report.
#[derive(Debug, Clone, PartialEq)]
pub struct TaxReport {
    /// Number of trades scanned.
    pub scanned_trade_count: usize,
    /// Number of closed trades that could be classified by holding period.
    pub taxable_trade_count: usize,
    /// Number of closed trades skipped because fill dates were missing.
    pub unclassified_trade_count: usize,
    /// Number of wash-sale allocation rows considered.
    pub wash_sale_adjustment_count: usize,
    /// Gross realized gains before adjustments.
    pub gross_realized_gain: Decimal,
    /// Gross realized losses before adjustments.
    pub gross_realized_loss: Decimal,
    /// Wash-sale losses disallowed across loss trades.
    pub wash_sale_disallowed_loss: Decimal,
    /// Replacement basis adjustments applied to closed replacement trades.
    pub replacement_basis_adjustment: Decimal,
    /// Net taxable short-term gain/loss.
    pub short_term_taxable_gain: Decimal,
    /// Net taxable long-term gain/loss.
    pub long_term_taxable_gain: Decimal,
    /// Net taxable gain/loss across holding-period buckets.
    pub net_taxable_gain: Decimal,
    /// Sum of positive line-level tax estimates before category loss netting.
    pub estimated_gross_tax_liability: Decimal,
    /// Estimated liability after netting losses inside each holding-period bucket.
    pub estimated_net_tax_liability: Decimal,
    /// Per-account summaries.
    pub account_summaries: Vec<TaxAccountSummary>,
    /// Per-trade tax lines.
    pub trades: Vec<TaxTradeLine>,
}

/// Stateless tax report calculator.
#[derive(Debug, Default)]
pub struct TaxService;

impl TaxService {
    /// Calculate tax lines and summaries from a trade ledger and wash-sale report.
    pub fn calculate(
        trades: &[Trade],
        account_rates: &BTreeMap<Uuid, TaxRates>,
        wash_sales: &WashSaleReport,
    ) -> Result<TaxReport, TaxReportError> {
        let disallowed_by_loss_trade = disallowed_loss_by_trade(wash_sales)?;
        let replacement_basis_by_trade = replacement_basis_adjustment_by_trade(wash_sales)?;
        let closed_trade_count = trades.iter().filter(|trade| is_closed(trade)).count();

        let mut lines = Vec::new();
        let mut unclassified_trade_count: usize = 0;
        for trade in trades.iter().filter(|trade| is_closed(trade)) {
            match tax_line(
                trade,
                account_rates,
                &disallowed_by_loss_trade,
                &replacement_basis_by_trade,
            )? {
                Some(line) => lines.push(line),
                None => {
                    unclassified_trade_count =
                        unclassified_trade_count.checked_add(1).ok_or_else(|| {
                            TaxReportError::calculation("unclassified trade count overflow")
                        })?;
                }
            }
        }

        lines.sort_by_key(|line| (line.closed_date, line.account_id, line.trade_id));
        let account_summaries = summarize_accounts(&lines, account_rates)?;
        let totals = summarize_lines(&lines)?;
        let net_taxable_gain = totals.net_taxable_gain()?;
        let estimated_net_tax_liability = sum_decimal(
            account_summaries
                .iter()
                .map(|summary| summary.estimated_net_tax_liability),
        )?;

        Ok(TaxReport {
            scanned_trade_count: closed_trade_count,
            taxable_trade_count: lines.len(),
            unclassified_trade_count,
            wash_sale_adjustment_count: wash_sales.matches.len(),
            gross_realized_gain: totals.gross_realized_gain,
            gross_realized_loss: totals.gross_realized_loss,
            wash_sale_disallowed_loss: totals.wash_sale_disallowed_loss,
            replacement_basis_adjustment: totals.replacement_basis_adjustment,
            short_term_taxable_gain: totals.short_term_taxable_gain,
            long_term_taxable_gain: totals.long_term_taxable_gain,
            net_taxable_gain,
            estimated_gross_tax_liability: totals.estimated_gross_tax_liability,
            estimated_net_tax_liability,
            account_summaries,
            trades: lines,
        })
    }
}

#[derive(Debug, Clone)]
struct TaxSummaryAccumulator {
    gross_realized_gain: Decimal,
    gross_realized_loss: Decimal,
    wash_sale_disallowed_loss: Decimal,
    replacement_basis_adjustment: Decimal,
    short_term_taxable_gain: Decimal,
    long_term_taxable_gain: Decimal,
    estimated_gross_tax_liability: Decimal,
}

impl Default for TaxSummaryAccumulator {
    fn default() -> Self {
        Self {
            gross_realized_gain: Decimal::ZERO,
            gross_realized_loss: Decimal::ZERO,
            wash_sale_disallowed_loss: Decimal::ZERO,
            replacement_basis_adjustment: Decimal::ZERO,
            short_term_taxable_gain: Decimal::ZERO,
            long_term_taxable_gain: Decimal::ZERO,
            estimated_gross_tax_liability: Decimal::ZERO,
        }
    }
}

impl TaxSummaryAccumulator {
    fn add_line(&mut self, line: &TaxTradeLine) -> Result<(), TaxReportError> {
        if line.realized_gain_loss >= Decimal::ZERO {
            self.gross_realized_gain = checked_add(
                self.gross_realized_gain,
                line.realized_gain_loss,
                "gross realized gain",
            )?;
        } else {
            self.gross_realized_loss = checked_add(
                self.gross_realized_loss,
                line.realized_gain_loss,
                "gross realized loss",
            )?;
        }
        self.wash_sale_disallowed_loss = checked_add(
            self.wash_sale_disallowed_loss,
            line.wash_sale_disallowed_loss,
            "wash sale disallowed loss",
        )?;
        self.replacement_basis_adjustment = checked_add(
            self.replacement_basis_adjustment,
            line.replacement_basis_adjustment,
            "replacement basis adjustment",
        )?;
        match line.holding_period {
            TaxHoldingPeriod::ShortTerm => {
                self.short_term_taxable_gain = checked_add(
                    self.short_term_taxable_gain,
                    line.taxable_gain_loss,
                    "short-term taxable gain",
                )?;
            }
            TaxHoldingPeriod::LongTerm => {
                self.long_term_taxable_gain = checked_add(
                    self.long_term_taxable_gain,
                    line.taxable_gain_loss,
                    "long-term taxable gain",
                )?;
            }
        }
        self.estimated_gross_tax_liability = checked_add(
            self.estimated_gross_tax_liability,
            line.estimated_tax_liability,
            "gross tax liability",
        )?;
        Ok(())
    }

    fn net_taxable_gain(&self) -> Result<Decimal, TaxReportError> {
        checked_add(
            self.short_term_taxable_gain,
            self.long_term_taxable_gain,
            "net taxable gain",
        )
    }

    fn estimated_net_tax_liability(&self, rates: TaxRates) -> Result<Decimal, TaxReportError> {
        let short_tax = positive_tax(self.short_term_taxable_gain, rates.short_term)?;
        let long_tax = positive_tax(self.long_term_taxable_gain, rates.long_term)?;
        checked_add(short_tax, long_tax, "net tax liability")
    }
}

fn tax_line(
    trade: &Trade,
    account_rates: &BTreeMap<Uuid, TaxRates>,
    disallowed_by_loss_trade: &BTreeMap<Uuid, Decimal>,
    replacement_basis_by_trade: &BTreeMap<Uuid, Decimal>,
) -> Result<Option<TaxTradeLine>, TaxReportError> {
    let Some(opened_date) = entry_date(trade) else {
        return Ok(None);
    };
    let Some(closed_date) = exit_date(trade) else {
        return Ok(None);
    };
    let rates = *account_rates.get(&trade.account_id).unwrap_or(&TaxRates {
        short_term: Decimal::ZERO,
        long_term: Decimal::ZERO,
    });
    let holding_days = closed_date.signed_duration_since(opened_date).num_days();
    let holding_period = holding_period(opened_date, closed_date)?;
    let tax_rate = match holding_period {
        TaxHoldingPeriod::ShortTerm => rates.short_term,
        TaxHoldingPeriod::LongTerm => rates.long_term,
    };
    let wash_sale_disallowed_loss = disallowed_by_loss_trade
        .get(&trade.id)
        .copied()
        .unwrap_or(Decimal::ZERO);
    let replacement_basis_adjustment = replacement_basis_by_trade
        .get(&trade.id)
        .copied()
        .unwrap_or(Decimal::ZERO);
    let taxable_gain_loss = checked_sub(
        checked_add(
            trade.balance.total_performance,
            wash_sale_disallowed_loss,
            "taxable gain wash sale adjustment",
        )?,
        replacement_basis_adjustment,
        "taxable gain replacement basis adjustment",
    )?;
    let estimated_tax_liability = positive_tax(taxable_gain_loss, tax_rate)?;

    Ok(Some(TaxTradeLine {
        account_id: trade.account_id,
        trade_id: trade.id,
        symbol: normalized_symbol(trade),
        opened_date,
        closed_date,
        holding_days,
        holding_period,
        realized_gain_loss: trade.balance.total_performance,
        wash_sale_disallowed_loss,
        replacement_basis_adjustment,
        taxable_gain_loss,
        tax_rate,
        estimated_tax_liability,
    }))
}

fn summarize_accounts(
    lines: &[TaxTradeLine],
    account_rates: &BTreeMap<Uuid, TaxRates>,
) -> Result<Vec<TaxAccountSummary>, TaxReportError> {
    let mut grouped: BTreeMap<Uuid, (TaxSummaryAccumulator, usize)> = BTreeMap::new();
    for line in lines {
        let entry = grouped.entry(line.account_id).or_default();
        entry.0.add_line(line)?;
        entry.1 = entry
            .1
            .checked_add(1)
            .ok_or_else(|| TaxReportError::calculation("account trade count overflow"))?;
    }

    grouped
        .into_iter()
        .map(|(account_id, (summary, trade_count))| {
            let rates = *account_rates.get(&account_id).unwrap_or(&TaxRates {
                short_term: Decimal::ZERO,
                long_term: Decimal::ZERO,
            });
            let net_taxable_gain = summary.net_taxable_gain()?;
            let estimated_net_tax_liability = summary.estimated_net_tax_liability(rates)?;
            Ok(TaxAccountSummary {
                account_id,
                short_term_rate: rates.short_term,
                long_term_rate: rates.long_term,
                trade_count,
                gross_realized_gain: summary.gross_realized_gain,
                gross_realized_loss: summary.gross_realized_loss,
                wash_sale_disallowed_loss: summary.wash_sale_disallowed_loss,
                replacement_basis_adjustment: summary.replacement_basis_adjustment,
                short_term_taxable_gain: summary.short_term_taxable_gain,
                long_term_taxable_gain: summary.long_term_taxable_gain,
                net_taxable_gain,
                estimated_gross_tax_liability: summary.estimated_gross_tax_liability,
                estimated_net_tax_liability,
            })
        })
        .collect()
}

fn summarize_lines(lines: &[TaxTradeLine]) -> Result<TaxSummaryAccumulator, TaxReportError> {
    let mut summary = TaxSummaryAccumulator::default();
    for line in lines {
        summary.add_line(line)?;
    }
    Ok(summary)
}

fn disallowed_loss_by_trade(
    wash_sales: &WashSaleReport,
) -> Result<BTreeMap<Uuid, Decimal>, TaxReportError> {
    let mut grouped = BTreeMap::new();
    for wash_sale in &wash_sales.matches {
        let current = grouped
            .get(&wash_sale.loss_trade_id)
            .copied()
            .unwrap_or(Decimal::ZERO);
        grouped.insert(
            wash_sale.loss_trade_id,
            checked_add(
                current,
                wash_sale.disallowed_loss,
                "wash sale loss grouping",
            )?,
        );
    }
    Ok(grouped)
}

fn replacement_basis_adjustment_by_trade(
    wash_sales: &WashSaleReport,
) -> Result<BTreeMap<Uuid, Decimal>, TaxReportError> {
    let mut grouped = BTreeMap::new();
    for adjustment in &wash_sales.replacement_adjustments {
        let current = grouped
            .get(&adjustment.replacement_trade_id)
            .copied()
            .unwrap_or(Decimal::ZERO);
        grouped.insert(
            adjustment.replacement_trade_id,
            checked_add(
                current,
                adjustment.basis_adjustment,
                "wash sale replacement basis grouping",
            )?,
        );
    }
    Ok(grouped)
}

fn validate_rate(rate: Decimal) -> Result<(), TaxReportError> {
    if rate < Decimal::ZERO || rate > Decimal::ONE {
        return Err(TaxReportError::calculation("tax rate outside 0..=1"));
    }
    Ok(())
}

fn is_closed(trade: &Trade) -> bool {
    matches!(trade.status, Status::ClosedTarget | Status::ClosedStopLoss)
}

fn normalized_symbol(trade: &Trade) -> String {
    trade.trading_vehicle.symbol.trim().to_uppercase()
}

fn entry_date(trade: &Trade) -> Option<NaiveDate> {
    trade.entry.filled_at.map(|filled_at| filled_at.date())
}

fn exit_date(trade: &Trade) -> Option<NaiveDate> {
    match trade.status {
        Status::ClosedTarget => trade.target.filled_at.map(|filled_at| filled_at.date()),
        Status::ClosedStopLoss => trade
            .safety_stop
            .filled_at
            .map(|filled_at| filled_at.date()),
        _ => None,
    }
}

fn holding_period(
    opened_date: NaiveDate,
    closed_date: NaiveDate,
) -> Result<TaxHoldingPeriod, TaxReportError> {
    let long_term_start = opened_date
        .checked_add_days(Days::new(LONG_TERM_THRESHOLD_DAYS))
        .ok_or_else(|| TaxReportError::calculation("holding-period date overflow"))?;
    if closed_date >= long_term_start {
        Ok(TaxHoldingPeriod::LongTerm)
    } else {
        Ok(TaxHoldingPeriod::ShortTerm)
    }
}

fn positive_tax(amount: Decimal, rate: Decimal) -> Result<Decimal, TaxReportError> {
    if amount <= Decimal::ZERO {
        return Ok(Decimal::ZERO);
    }
    checked_mul(amount, rate, "tax liability")
}

fn checked_add(
    left: Decimal,
    right: Decimal,
    context: &'static str,
) -> Result<Decimal, TaxReportError> {
    left.checked_add(right)
        .ok_or_else(|| TaxReportError::calculation(context))
}

fn checked_sub(
    left: Decimal,
    right: Decimal,
    context: &'static str,
) -> Result<Decimal, TaxReportError> {
    left.checked_sub(right)
        .ok_or_else(|| TaxReportError::calculation(context))
}

fn checked_mul(
    left: Decimal,
    right: Decimal,
    context: &'static str,
) -> Result<Decimal, TaxReportError> {
    left.checked_mul(right)
        .ok_or_else(|| TaxReportError::calculation(context))
}

fn sum_decimal(mut values: impl Iterator<Item = Decimal>) -> Result<Decimal, TaxReportError> {
    values.try_fold(Decimal::ZERO, |acc, value| {
        checked_add(acc, value, "decimal sum")
    })
}

#[cfg(test)]
mod tests {
    use super::{TaxHoldingPeriod, TaxRates, TaxService};
    use crate::services::{
        WashSaleMatch, WashSaleReplacementAdjustment, WashSaleReport, WashSaleService,
    };
    use chrono::{NaiveDate, NaiveDateTime};
    use model::{
        Currency, Order, OrderStatus, Status, Trade, TradeCategory, TradingVehicle,
        TradingVehicleCategory,
    };
    use rust_decimal::Decimal;
    use rust_decimal_macros::dec;
    use std::collections::BTreeMap;
    use uuid::Uuid;

    fn date(year: i32, month: u32, day: u32) -> NaiveDateTime {
        NaiveDate::from_ymd_opt(year, month, day)
            .expect("valid date")
            .and_hms_opt(14, 30, 0)
            .expect("valid time")
    }

    fn filled_order(quantity: Decimal, price: Decimal, filled_at: NaiveDateTime) -> Order {
        Order {
            quantity,
            filled_quantity: quantity,
            average_filled_price: Some(price),
            unit_price: price,
            status: OrderStatus::Filled,
            filled_at: Some(filled_at),
            currency: Currency::USD,
            ..Default::default()
        }
    }

    struct TradeSpec {
        account_id: Uuid,
        symbol: &'static str,
        status: Status,
        quantity: Decimal,
        entry_price: Decimal,
        entry_day: NaiveDateTime,
        exit_price: Decimal,
        exit_day: NaiveDateTime,
        pnl: Decimal,
    }

    fn trade(spec: TradeSpec) -> Trade {
        let entry = filled_order(spec.quantity, spec.entry_price, spec.entry_day);
        let exit = filled_order(spec.quantity, spec.exit_price, spec.exit_day);
        let (target, safety_stop) = match spec.status {
            Status::ClosedTarget => (exit, Order::default()),
            Status::ClosedStopLoss => (Order::default(), exit),
            _ => (Order::default(), Order::default()),
        };

        Trade {
            id: Uuid::new_v4(),
            account_id: spec.account_id,
            trading_vehicle: TradingVehicle {
                symbol: spec.symbol.to_string(),
                category: TradingVehicleCategory::Stock,
                ..Default::default()
            },
            category: TradeCategory::Long,
            status: spec.status,
            currency: Currency::USD,
            entry,
            safety_stop,
            target,
            balance: model::TradeBalance {
                total_performance: spec.pnl,
                ..Default::default()
            },
            ..Default::default()
        }
    }

    fn rates(account_id: Uuid, short: Decimal, long: Decimal) -> BTreeMap<Uuid, TaxRates> {
        BTreeMap::from([(account_id, TaxRates::new(short, long).expect("valid rates"))])
    }

    fn wash_sale_report(
        account_id: Uuid,
        loss_trade_id: Uuid,
        replacement_trade_id: Uuid,
    ) -> WashSaleReport {
        WashSaleReport {
            scanned_trade_count: 2,
            eligible_loss_trade_count: 1,
            wash_sale_count: 1,
            total_disallowed_loss: dec!(50),
            total_basis_adjustment: dec!(50),
            matches: vec![WashSaleMatch {
                account_id,
                symbol: "AAPL".to_string(),
                loss_trade_id,
                replacement_trade_id,
                sale_date: NaiveDate::from_ymd_opt(2026, 1, 10).expect("sale date"),
                replacement_purchase_date: NaiveDate::from_ymd_opt(2026, 1, 20)
                    .expect("replacement date"),
                loss_quantity: dec!(10),
                matched_quantity: dec!(5),
                realized_loss: dec!(100),
                disallowed_loss: dec!(50),
                replacement_cost_basis: dec!(475),
                adjusted_replacement_cost_basis: dec!(525),
            }],
            replacement_adjustments: vec![WashSaleReplacementAdjustment {
                replacement_trade_id,
                account_id,
                symbol: "AAPL".to_string(),
                replacement_purchase_date: NaiveDate::from_ymd_opt(2026, 1, 20)
                    .expect("replacement date"),
                matched_quantity: dec!(5),
                original_cost_basis: dec!(475),
                basis_adjustment: dec!(50),
                adjusted_cost_basis: dec!(525),
            }],
        }
    }

    #[test]
    fn classifies_short_and_long_term_trades_and_nets_losses() {
        let account_id = Uuid::new_v4();
        let short_gain = trade(TradeSpec {
            account_id,
            symbol: "AAPL",
            status: Status::ClosedTarget,
            quantity: dec!(10),
            entry_price: dec!(100),
            entry_day: date(2026, 1, 1),
            exit_price: dec!(110),
            exit_day: date(2026, 2, 1),
            pnl: dec!(100),
        });
        let short_loss = trade(TradeSpec {
            account_id,
            symbol: "MSFT",
            status: Status::ClosedStopLoss,
            quantity: dec!(10),
            entry_price: dec!(100),
            entry_day: date(2026, 1, 1),
            exit_price: dec!(95),
            exit_day: date(2026, 3, 1),
            pnl: dec!(-50),
        });
        let long_gain = trade(TradeSpec {
            account_id,
            symbol: "GOOG",
            status: Status::ClosedTarget,
            quantity: dec!(10),
            entry_price: dec!(100),
            entry_day: date(2024, 1, 1),
            exit_price: dec!(120),
            exit_day: date(2025, 1, 3),
            pnl: dec!(200),
        });
        let wash_sales = WashSaleService::detect(&[]).expect("empty wash sale report");

        let report = TaxService::calculate(
            &[short_gain, short_loss, long_gain],
            &rates(account_id, dec!(0.30), dec!(0.15)),
            &wash_sales,
        )
        .expect("tax report");

        assert_eq!(report.taxable_trade_count, 3);
        assert_eq!(report.unclassified_trade_count, 0);
        assert_eq!(report.short_term_taxable_gain, dec!(50));
        assert_eq!(report.long_term_taxable_gain, dec!(200));
        assert_eq!(report.net_taxable_gain, dec!(250));
        assert_eq!(report.estimated_gross_tax_liability, dec!(60.00));
        assert_eq!(report.estimated_net_tax_liability, dec!(45.00));
        let long_line = report
            .trades
            .iter()
            .find(|line| line.symbol == "GOOG")
            .expect("long-term line");
        assert_eq!(long_line.holding_period, TaxHoldingPeriod::LongTerm);
    }

    #[test]
    fn applies_wash_sale_loss_and_replacement_basis_adjustments() {
        let account_id = Uuid::new_v4();
        let loss_trade_id = Uuid::new_v4();
        let replacement_trade_id = Uuid::new_v4();
        let mut loss = trade(TradeSpec {
            account_id,
            symbol: "AAPL",
            status: Status::ClosedStopLoss,
            quantity: dec!(10),
            entry_price: dec!(100),
            entry_day: date(2026, 1, 1),
            exit_price: dec!(90),
            exit_day: date(2026, 1, 10),
            pnl: dec!(-100),
        });
        loss.id = loss_trade_id;
        let mut replacement = trade(TradeSpec {
            account_id,
            symbol: "AAPL",
            status: Status::ClosedTarget,
            quantity: dec!(5),
            entry_price: dec!(95),
            entry_day: date(2026, 1, 20),
            exit_price: dec!(140),
            exit_day: date(2026, 2, 20),
            pnl: dec!(225),
        });
        replacement.id = replacement_trade_id;
        let wash_sales = wash_sale_report(account_id, loss_trade_id, replacement_trade_id);

        let report = TaxService::calculate(
            &[loss, replacement],
            &rates(account_id, dec!(0.25), dec!(0.15)),
            &wash_sales,
        )
        .expect("tax report");

        let loss_line = report
            .trades
            .iter()
            .find(|line| line.trade_id == loss_trade_id)
            .expect("loss line");
        assert_eq!(loss_line.taxable_gain_loss, dec!(-50));
        assert_eq!(loss_line.wash_sale_disallowed_loss, dec!(50));

        let replacement_line = report
            .trades
            .iter()
            .find(|line| line.trade_id == replacement_trade_id)
            .expect("replacement line");
        assert_eq!(replacement_line.replacement_basis_adjustment, dec!(50));
        assert_eq!(replacement_line.taxable_gain_loss, dec!(175));
        assert_eq!(report.short_term_taxable_gain, dec!(125));
        assert_eq!(report.estimated_net_tax_liability, dec!(31.25));
    }
}
