//! Wash sale detection and basis adjustment reporting.

use chrono::{Days, NaiveDate};
use model::{Status, Trade, TradeCategory, TradingVehicleCategory};
use rust_decimal::Decimal;
use std::collections::BTreeMap;
use std::error::Error;
use std::fmt::{Display, Formatter};
use uuid::Uuid;

const WASH_SALE_WINDOW_DAYS: u64 = 30;

/// Error returned when wash sale analysis cannot be computed safely.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WashSaleError {
    context: &'static str,
}

impl WashSaleError {
    fn calculation(context: &'static str) -> Self {
        Self { context }
    }
}

impl Display for WashSaleError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "wash sale calculation failed: {}", self.context)
    }
}

impl Error for WashSaleError {}

/// A single loss-sale to replacement-purchase wash sale allocation.
#[derive(Debug, Clone, PartialEq)]
pub struct WashSaleMatch {
    /// Account owning both the loss sale and replacement trade.
    pub account_id: Uuid,
    /// Normalized symbol that matched between the sale and replacement.
    pub symbol: String,
    /// Trade that realized the loss.
    pub loss_trade_id: Uuid,
    /// Trade that bought replacement shares or units.
    pub replacement_trade_id: Uuid,
    /// Loss sale date.
    pub sale_date: NaiveDate,
    /// Replacement purchase date.
    pub replacement_purchase_date: NaiveDate,
    /// Quantity sold at a loss.
    pub loss_quantity: Decimal,
    /// Quantity matched to this replacement.
    pub matched_quantity: Decimal,
    /// Total realized loss on the loss trade before wash-sale allocation.
    pub realized_loss: Decimal,
    /// Loss disallowed for this matched replacement quantity.
    pub disallowed_loss: Decimal,
    /// Replacement cost basis before wash-sale adjustment for the matched quantity.
    pub replacement_cost_basis: Decimal,
    /// Adjusted replacement cost basis for the matched quantity.
    pub adjusted_replacement_cost_basis: Decimal,
}

/// Aggregated cost-basis adjustment for one replacement trade.
#[derive(Debug, Clone, PartialEq)]
pub struct WashSaleReplacementAdjustment {
    /// Replacement trade whose cost basis is adjusted.
    pub replacement_trade_id: Uuid,
    /// Account owning the replacement trade.
    pub account_id: Uuid,
    /// Symbol for the replacement trade.
    pub symbol: String,
    /// Purchase date for the replacement trade.
    pub replacement_purchase_date: NaiveDate,
    /// Quantity matched across all loss-sale allocations.
    pub matched_quantity: Decimal,
    /// Original cost basis for the matched replacement quantity.
    pub original_cost_basis: Decimal,
    /// Disallowed loss added to replacement basis.
    pub basis_adjustment: Decimal,
    /// Adjusted cost basis for the matched replacement quantity.
    pub adjusted_cost_basis: Decimal,
}

/// Wash sale analysis result for a trade set.
#[derive(Debug, Clone, PartialEq)]
pub struct WashSaleReport {
    /// Number of trades scanned.
    pub scanned_trade_count: usize,
    /// Number of eligible loss trades found before allocation.
    pub eligible_loss_trade_count: usize,
    /// Number of loss-sale to replacement-purchase allocations.
    pub wash_sale_count: usize,
    /// Sum of all disallowed losses.
    pub total_disallowed_loss: Decimal,
    /// Sum of all replacement basis adjustments.
    pub total_basis_adjustment: Decimal,
    /// Individual wash sale allocations.
    pub matches: Vec<WashSaleMatch>,
    /// Replacement-trade cost-basis adjustments.
    pub replacement_adjustments: Vec<WashSaleReplacementAdjustment>,
}

/// Stateless wash sale detector.
#[derive(Debug, Default)]
pub struct WashSaleService;

impl WashSaleService {
    /// Detect wash sale allocations for the provided trade ledger.
    pub fn detect(trades: &[Trade]) -> Result<WashSaleReport, WashSaleError> {
        let mut losses = loss_lots(trades)?;
        let mut replacements = replacement_lots(trades);
        losses.sort_by_key(|lot| (lot.sale_date, lot.trade_id));
        replacements.sort_by_key(|lot| (lot.purchase_date, lot.trade_id));

        let mut matches = Vec::new();
        for loss in &mut losses {
            allocate_loss_to_replacements(loss, &mut replacements, &mut matches)?;
        }

        let replacement_adjustments = aggregate_replacement_adjustments(&matches)?;
        let total_disallowed_loss = sum_decimal(matches.iter().map(|item| item.disallowed_loss))?;
        let total_basis_adjustment = sum_decimal(
            replacement_adjustments
                .iter()
                .map(|item| item.basis_adjustment),
        )?;
        let wash_sale_count = matches.len();

        Ok(WashSaleReport {
            scanned_trade_count: trades.len(),
            eligible_loss_trade_count: losses.len(),
            wash_sale_count,
            total_disallowed_loss,
            total_basis_adjustment,
            matches,
            replacement_adjustments,
        })
    }
}

#[derive(Debug, Clone)]
struct LossLot {
    account_id: Uuid,
    trade_id: Uuid,
    symbol: String,
    sale_date: NaiveDate,
    quantity: Decimal,
    remaining_quantity: Decimal,
    realized_loss: Decimal,
}

#[derive(Debug, Clone)]
struct ReplacementLot {
    account_id: Uuid,
    trade_id: Uuid,
    symbol: String,
    purchase_date: NaiveDate,
    exit_date: Option<NaiveDate>,
    remaining_quantity: Decimal,
    cost_basis_per_unit: Decimal,
}

fn loss_lots(trades: &[Trade]) -> Result<Vec<LossLot>, WashSaleError> {
    trades
        .iter()
        .filter_map(loss_lot)
        .collect::<Result<Vec<_>, _>>()
}

fn loss_lot(trade: &Trade) -> Option<Result<LossLot, WashSaleError>> {
    if !is_wash_sale_eligible_trade(trade) || trade.category != TradeCategory::Long {
        return None;
    }
    if !matches!(trade.status, Status::ClosedStopLoss | Status::ClosedTarget) {
        return None;
    }
    if trade.balance.total_performance >= Decimal::ZERO {
        return None;
    }

    let quantity = exit_quantity(trade);
    if quantity <= Decimal::ZERO {
        return None;
    }

    let sale_date = exit_date(trade)?;

    Some(
        Decimal::ZERO
            .checked_sub(trade.balance.total_performance)
            .ok_or_else(|| WashSaleError::calculation("realized loss overflow"))
            .map(|realized_loss| LossLot {
                account_id: trade.account_id,
                trade_id: trade.id,
                symbol: normalized_symbol(trade),
                sale_date,
                quantity,
                remaining_quantity: quantity,
                realized_loss,
            }),
    )
}

fn replacement_lots(trades: &[Trade]) -> Vec<ReplacementLot> {
    trades.iter().filter_map(replacement_lot).collect()
}

fn replacement_lot(trade: &Trade) -> Option<ReplacementLot> {
    if !is_wash_sale_eligible_trade(trade) || trade.category != TradeCategory::Long {
        return None;
    }

    let quantity = entry_quantity(trade);
    if quantity <= Decimal::ZERO {
        return None;
    }

    let purchase_date = entry_date(trade)?;
    let price = trade
        .entry
        .average_filled_price
        .unwrap_or(trade.entry.unit_price);
    if price <= Decimal::ZERO {
        return None;
    }

    Some(ReplacementLot {
        account_id: trade.account_id,
        trade_id: trade.id,
        symbol: normalized_symbol(trade),
        purchase_date,
        exit_date: exit_date(trade),
        remaining_quantity: quantity,
        cost_basis_per_unit: price,
    })
}

fn allocate_loss_to_replacements(
    loss: &mut LossLot,
    replacements: &mut [ReplacementLot],
    matches: &mut Vec<WashSaleMatch>,
) -> Result<(), WashSaleError> {
    for replacement in replacements {
        if loss.remaining_quantity <= Decimal::ZERO {
            break;
        }
        if !can_match(loss, replacement)? {
            continue;
        }

        let matched_quantity = min_decimal(loss.remaining_quantity, replacement.remaining_quantity);
        if matched_quantity <= Decimal::ZERO {
            continue;
        }

        let disallowed_loss = prorate_loss(loss.realized_loss, loss.quantity, matched_quantity)?;
        let replacement_cost_basis = checked_mul(
            replacement.cost_basis_per_unit,
            matched_quantity,
            "replacement basis",
        )?;
        let adjusted_replacement_cost_basis = checked_add(
            replacement_cost_basis,
            disallowed_loss,
            "adjusted replacement basis",
        )?;

        loss.remaining_quantity = checked_sub(
            loss.remaining_quantity,
            matched_quantity,
            "loss remaining quantity",
        )?;
        replacement.remaining_quantity = checked_sub(
            replacement.remaining_quantity,
            matched_quantity,
            "replacement remaining quantity",
        )?;

        matches.push(WashSaleMatch {
            account_id: loss.account_id,
            symbol: loss.symbol.clone(),
            loss_trade_id: loss.trade_id,
            replacement_trade_id: replacement.trade_id,
            sale_date: loss.sale_date,
            replacement_purchase_date: replacement.purchase_date,
            loss_quantity: loss.quantity,
            matched_quantity,
            realized_loss: loss.realized_loss,
            disallowed_loss,
            replacement_cost_basis,
            adjusted_replacement_cost_basis,
        });
    }
    Ok(())
}

fn can_match(loss: &LossLot, replacement: &ReplacementLot) -> Result<bool, WashSaleError> {
    if loss.account_id != replacement.account_id
        || loss.trade_id == replacement.trade_id
        || loss.symbol != replacement.symbol
        || replacement.remaining_quantity <= Decimal::ZERO
    {
        return Ok(false);
    }

    let start = loss
        .sale_date
        .checked_sub_days(Days::new(WASH_SALE_WINDOW_DAYS))
        .ok_or_else(|| WashSaleError::calculation("wash sale window start overflow"))?;
    let end = loss
        .sale_date
        .checked_add_days(Days::new(WASH_SALE_WINDOW_DAYS))
        .ok_or_else(|| WashSaleError::calculation("wash sale window end overflow"))?;
    if replacement.purchase_date < start || replacement.purchase_date > end {
        return Ok(false);
    }

    if replacement.purchase_date <= loss.sale_date {
        return Ok(replacement
            .exit_date
            .map(|exit| exit >= loss.sale_date)
            .unwrap_or(true));
    }

    Ok(true)
}

fn aggregate_replacement_adjustments(
    matches: &[WashSaleMatch],
) -> Result<Vec<WashSaleReplacementAdjustment>, WashSaleError> {
    let mut grouped: BTreeMap<Uuid, WashSaleReplacementAdjustment> = BTreeMap::new();
    for wash_sale in matches {
        let entry = grouped.entry(wash_sale.replacement_trade_id).or_insert(
            WashSaleReplacementAdjustment {
                replacement_trade_id: wash_sale.replacement_trade_id,
                account_id: wash_sale.account_id,
                symbol: wash_sale.symbol.clone(),
                replacement_purchase_date: wash_sale.replacement_purchase_date,
                matched_quantity: Decimal::ZERO,
                original_cost_basis: Decimal::ZERO,
                basis_adjustment: Decimal::ZERO,
                adjusted_cost_basis: Decimal::ZERO,
            },
        );
        entry.matched_quantity = checked_add(
            entry.matched_quantity,
            wash_sale.matched_quantity,
            "aggregate matched quantity",
        )?;
        entry.original_cost_basis = checked_add(
            entry.original_cost_basis,
            wash_sale.replacement_cost_basis,
            "aggregate original basis",
        )?;
        entry.basis_adjustment = checked_add(
            entry.basis_adjustment,
            wash_sale.disallowed_loss,
            "aggregate basis adjustment",
        )?;
        entry.adjusted_cost_basis = checked_add(
            entry.adjusted_cost_basis,
            wash_sale.adjusted_replacement_cost_basis,
            "aggregate adjusted basis",
        )?;
    }
    Ok(grouped.into_values().collect())
}

fn is_wash_sale_eligible_trade(trade: &Trade) -> bool {
    matches!(
        trade.trading_vehicle.category,
        TradingVehicleCategory::Stock | TradingVehicleCategory::Etf | TradingVehicleCategory::Bond
    )
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

fn entry_quantity(trade: &Trade) -> Decimal {
    if trade.entry.filled_quantity > Decimal::ZERO {
        trade.entry.filled_quantity
    } else {
        Decimal::ZERO
    }
}

fn exit_quantity(trade: &Trade) -> Decimal {
    let order = match trade.status {
        Status::ClosedTarget => &trade.target,
        Status::ClosedStopLoss => &trade.safety_stop,
        _ => return Decimal::ZERO,
    };
    if order.filled_quantity > Decimal::ZERO {
        order.filled_quantity
    } else {
        Decimal::ZERO
    }
}

fn prorate_loss(
    realized_loss: Decimal,
    loss_quantity: Decimal,
    matched_quantity: Decimal,
) -> Result<Decimal, WashSaleError> {
    let ratio = matched_quantity
        .checked_div(loss_quantity)
        .ok_or_else(|| WashSaleError::calculation("loss quantity division"))?;
    checked_mul(realized_loss, ratio, "prorated loss")
}

fn sum_decimal(mut values: impl Iterator<Item = Decimal>) -> Result<Decimal, WashSaleError> {
    values.try_fold(Decimal::ZERO, |acc, value| {
        checked_add(acc, value, "decimal sum")
    })
}

fn min_decimal(left: Decimal, right: Decimal) -> Decimal {
    if left <= right {
        left
    } else {
        right
    }
}

fn checked_add(
    left: Decimal,
    right: Decimal,
    context: &'static str,
) -> Result<Decimal, WashSaleError> {
    left.checked_add(right)
        .ok_or_else(|| WashSaleError::calculation(context))
}

fn checked_sub(
    left: Decimal,
    right: Decimal,
    context: &'static str,
) -> Result<Decimal, WashSaleError> {
    left.checked_sub(right)
        .ok_or_else(|| WashSaleError::calculation(context))
}

fn checked_mul(
    left: Decimal,
    right: Decimal,
    context: &'static str,
) -> Result<Decimal, WashSaleError> {
    left.checked_mul(right)
        .ok_or_else(|| WashSaleError::calculation(context))
}

#[cfg(test)]
mod tests {
    use super::WashSaleService;
    use chrono::{NaiveDate, NaiveDateTime};
    use model::{
        Currency, Order, OrderStatus, Status, Trade, TradeCategory, TradingVehicle,
        TradingVehicleCategory,
    };
    use rust_decimal::Decimal;
    use rust_decimal_macros::dec;
    use uuid::Uuid;

    fn date(year: i32, month: u32, day: u32) -> chrono::NaiveDateTime {
        NaiveDate::from_ymd_opt(year, month, day)
            .expect("valid test date")
            .and_hms_opt(14, 30, 0)
            .expect("valid test time")
    }

    fn vehicle(symbol: &str, category: TradingVehicleCategory) -> TradingVehicle {
        TradingVehicle {
            symbol: symbol.to_string(),
            category,
            ..Default::default()
        }
    }

    fn filled_entry(quantity: Decimal, price: Decimal) -> Order {
        Order {
            quantity,
            filled_quantity: quantity,
            average_filled_price: Some(price),
            unit_price: price,
            status: OrderStatus::Filled,
            filled_at: Some(date(2026, 1, 1)),
            currency: Currency::USD,
            ..Default::default()
        }
    }

    struct TradeSpec {
        account_id: Uuid,
        symbol: &'static str,
        vehicle_category: TradingVehicleCategory,
        status: Status,
        quantity: Decimal,
        entry_price: Decimal,
        entry_day: NaiveDateTime,
        exit_price: Option<Decimal>,
        exit_day: Option<NaiveDateTime>,
        pnl: Decimal,
    }

    fn trade(spec: TradeSpec) -> Trade {
        let mut entry = filled_entry(spec.quantity, spec.entry_price);
        entry.filled_at = Some(spec.entry_day);
        let mut safety_stop = Order {
            quantity: spec.quantity,
            unit_price: spec.exit_price.unwrap_or(dec!(0)),
            currency: Currency::USD,
            ..Default::default()
        };
        let mut target = safety_stop.clone();
        if let (Some(price), Some(closed_at)) = (spec.exit_price, spec.exit_day) {
            let exit_order = Order {
                quantity: spec.quantity,
                filled_quantity: spec.quantity,
                average_filled_price: Some(price),
                unit_price: price,
                status: OrderStatus::Filled,
                filled_at: Some(closed_at),
                currency: Currency::USD,
                ..Default::default()
            };
            if spec.status == Status::ClosedTarget {
                target = exit_order;
            } else if spec.status == Status::ClosedStopLoss {
                safety_stop = exit_order;
            }
        }

        Trade {
            id: Uuid::new_v4(),
            account_id: spec.account_id,
            trading_vehicle: vehicle(spec.symbol, spec.vehicle_category),
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

    #[test]
    fn detects_same_symbol_repurchase_after_loss_and_adjusts_basis() {
        let account_id = Uuid::new_v4();
        let loss = trade(TradeSpec {
            account_id,
            symbol: "AAPL",
            vehicle_category: TradingVehicleCategory::Stock,
            status: Status::ClosedStopLoss,
            quantity: dec!(100),
            entry_price: dec!(50),
            entry_day: date(2026, 1, 1),
            exit_price: Some(dec!(45)),
            exit_day: Some(date(2026, 1, 10)),
            pnl: dec!(-500),
        });
        let replacement = trade(TradeSpec {
            account_id,
            symbol: "aapl",
            vehicle_category: TradingVehicleCategory::Stock,
            status: Status::Filled,
            quantity: dec!(100),
            entry_price: dec!(47),
            entry_day: date(2026, 1, 20),
            exit_price: None,
            exit_day: None,
            pnl: dec!(0),
        });

        let report = WashSaleService::detect(&[loss, replacement]).expect("report");

        assert_eq!(report.wash_sale_count, 1);
        assert_eq!(report.total_disallowed_loss, dec!(500));
        assert_eq!(report.total_basis_adjustment, dec!(500));
        let wash_sale = report.matches.first().expect("match");
        assert_eq!(wash_sale.symbol, "AAPL");
        assert_eq!(wash_sale.matched_quantity, dec!(100));
        assert_eq!(wash_sale.replacement_cost_basis, dec!(4700));
        assert_eq!(wash_sale.adjusted_replacement_cost_basis, dec!(5200));
        let adjustment = report
            .replacement_adjustments
            .first()
            .expect("replacement adjustment");
        assert_eq!(adjustment.basis_adjustment, dec!(500));
        assert_eq!(adjustment.adjusted_cost_basis, dec!(5200));
    }

    #[test]
    fn prorates_disallowed_loss_for_partial_replacement_quantity() {
        let account_id = Uuid::new_v4();
        let loss = trade(TradeSpec {
            account_id,
            symbol: "MSFT",
            vehicle_category: TradingVehicleCategory::Etf,
            status: Status::ClosedStopLoss,
            quantity: dec!(100),
            entry_price: dec!(100),
            entry_day: date(2026, 2, 1),
            exit_price: Some(dec!(90)),
            exit_day: Some(date(2026, 2, 5)),
            pnl: dec!(-1000),
        });
        let replacement = trade(TradeSpec {
            account_id,
            symbol: "MSFT",
            vehicle_category: TradingVehicleCategory::Etf,
            status: Status::Filled,
            quantity: dec!(40),
            entry_price: dec!(92),
            entry_day: date(2026, 2, 10),
            exit_price: None,
            exit_day: None,
            pnl: dec!(0),
        });

        let report = WashSaleService::detect(&[loss, replacement]).expect("report");

        assert_eq!(report.wash_sale_count, 1);
        assert_eq!(
            report.matches.first().expect("match").matched_quantity,
            dec!(40)
        );
        assert_eq!(report.total_disallowed_loss, dec!(400.0));
        assert_eq!(
            report
                .replacement_adjustments
                .first()
                .expect("adjustment")
                .adjusted_cost_basis,
            dec!(4080.0)
        );
    }

    #[test]
    fn detects_pre_sale_replacement_only_when_still_held_on_loss_sale_date() {
        let account_id = Uuid::new_v4();
        let loss = trade(TradeSpec {
            account_id,
            symbol: "TSLA",
            vehicle_category: TradingVehicleCategory::Stock,
            status: Status::ClosedStopLoss,
            quantity: dec!(50),
            entry_price: dec!(250),
            entry_day: date(2026, 3, 1),
            exit_price: Some(dec!(240)),
            exit_day: Some(date(2026, 3, 20)),
            pnl: dec!(-500),
        });
        let held_replacement = trade(TradeSpec {
            account_id,
            symbol: "TSLA",
            vehicle_category: TradingVehicleCategory::Stock,
            status: Status::Filled,
            quantity: dec!(30),
            entry_price: dec!(245),
            entry_day: date(2026, 3, 10),
            exit_price: None,
            exit_day: None,
            pnl: dec!(0),
        });
        let closed_before_sale = trade(TradeSpec {
            account_id,
            symbol: "TSLA",
            vehicle_category: TradingVehicleCategory::Stock,
            status: Status::ClosedTarget,
            quantity: dec!(30),
            entry_price: dec!(230),
            entry_day: date(2026, 3, 5),
            exit_price: Some(dec!(235)),
            exit_day: Some(date(2026, 3, 15)),
            pnl: dec!(150),
        });

        let report =
            WashSaleService::detect(&[loss, held_replacement, closed_before_sale]).expect("report");

        assert_eq!(report.wash_sale_count, 1);
        assert_eq!(report.total_disallowed_loss, dec!(300.0));
    }

    #[test]
    fn ignores_out_of_window_profit_crypto_short_and_cross_account_trades() {
        let account_id = Uuid::new_v4();
        let other_account_id = Uuid::new_v4();
        let loss = trade(TradeSpec {
            account_id,
            symbol: "QQQ",
            vehicle_category: TradingVehicleCategory::Stock,
            status: Status::ClosedStopLoss,
            quantity: dec!(10),
            entry_price: dec!(400),
            entry_day: date(2026, 4, 1),
            exit_price: Some(dec!(390)),
            exit_day: Some(date(2026, 4, 5)),
            pnl: dec!(-100),
        });
        let late_replacement = trade(TradeSpec {
            account_id,
            symbol: "SPY",
            vehicle_category: TradingVehicleCategory::Stock,
            status: Status::Filled,
            quantity: dec!(10),
            entry_price: dec!(395),
            entry_day: date(2026, 5, 10),
            exit_price: None,
            exit_day: None,
            pnl: dec!(0),
        });
        let cross_account = trade(TradeSpec {
            account_id: other_account_id,
            symbol: "SPY",
            vehicle_category: TradingVehicleCategory::Stock,
            status: Status::Filled,
            quantity: dec!(10),
            entry_price: dec!(395),
            entry_day: date(2026, 4, 10),
            exit_price: None,
            exit_day: None,
            pnl: dec!(0),
        });
        let crypto_loss = trade(TradeSpec {
            account_id,
            symbol: "BTCUSD",
            vehicle_category: TradingVehicleCategory::Crypto,
            status: Status::ClosedStopLoss,
            quantity: dec!(1),
            entry_price: dec!(60000),
            entry_day: date(2026, 4, 1),
            exit_price: Some(dec!(59000)),
            exit_day: Some(date(2026, 4, 5)),
            pnl: dec!(-1000),
        });
        let profit = trade(TradeSpec {
            account_id,
            symbol: "SPY",
            vehicle_category: TradingVehicleCategory::Stock,
            status: Status::ClosedTarget,
            quantity: dec!(10),
            entry_price: dec!(400),
            entry_day: date(2026, 4, 1),
            exit_price: Some(dec!(410)),
            exit_day: Some(date(2026, 4, 5)),
            pnl: dec!(100),
        });

        let report =
            WashSaleService::detect(&[loss, late_replacement, cross_account, crypto_loss, profit])
                .expect("report");

        assert_eq!(report.eligible_loss_trade_count, 1);
        assert_eq!(report.wash_sale_count, 0);
        assert_eq!(report.total_disallowed_loss, dec!(0));
    }
}
