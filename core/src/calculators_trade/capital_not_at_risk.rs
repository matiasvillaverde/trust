use model::{Currency, Order, OrderStatus, ReadTradeDB, Trade, TradeCategory};
use rust_decimal::Decimal;
use uuid::Uuid;

pub struct TradeCapitalNotAtRisk;

impl TradeCapitalNotAtRisk {
    /// Cash retained inside active trades plus proceeds at the planned stop,
    /// allowing for entry orders that can still fill. This is a planned liquidation
    /// value, not a guarantee against gaps, fees or failed protective orders.
    pub fn calculate(
        account_id: Uuid,
        currency: &Currency,
        database: &mut dyn ReadTradeDB,
    ) -> Result<Decimal, Box<dyn std::error::Error>> {
        database
            .all_open_trades_for_currency(account_id, currency)?
            .iter()
            .try_fold(Decimal::ZERO, |total, trade| {
                total
                    .checked_add(protected_capital(trade)?)
                    .ok_or_else(|| "Arithmetic overflow in addition of protected capital".into())
            })
    }
}

fn filled(order: &Order) -> Decimal {
    if (order.status == OrderStatus::Filled || order.filled_at.is_some())
        && order.filled_quantity.is_zero()
    {
        order.quantity
    } else {
        order.filled_quantity
    }
}

pub(crate) fn protected_capital(trade: &Trade) -> Result<Decimal, Box<dyn std::error::Error>> {
    let entered = filled(&trade.entry);
    let exited = filled(&trade.target)
        .checked_add(filled(&trade.safety_stop))
        .ok_or("Arithmetic overflow in addition of exit quantities")?;
    let remaining = entered
        .checked_sub(exited)
        .ok_or("Arithmetic overflow in subtraction of exit quantities")?;
    if entered < Decimal::ZERO || entered > trade.entry.quantity || remaining < Decimal::ZERO {
        return Err("Invalid cumulative quantities in open risk calculation".into());
    }
    let pending = if entered == trade.entry.quantity
        || matches!(
            trade.entry.status,
            OrderStatus::Filled
                | OrderStatus::Canceled
                | OrderStatus::Expired
                | OrderStatus::Rejected
        ) {
        Decimal::ZERO
    } else {
        trade
            .entry
            .quantity
            .checked_sub(entered)
            .ok_or("Arithmetic overflow in subtraction of pending quantity")?
    };
    let exposure = remaining
        .checked_add(pending)
        .ok_or("Arithmetic overflow in addition of exposure")?;
    let pending_notional = trade
        .entry
        .unit_price
        .checked_mul(pending)
        .ok_or("Arithmetic overflow in multiplication of pending notional")?;
    let stop_notional = trade
        .safety_stop
        .unit_price
        .checked_mul(exposure)
        .ok_or("Arithmetic overflow in multiplication of stop notional")?;
    // Shorts retain collateral AND sale proceeds; the stop is a cash debit.
    let change = match trade.category {
        TradeCategory::Long => stop_notional.checked_sub(pending_notional),
        TradeCategory::Short => pending_notional.checked_sub(stop_notional),
    }
    .ok_or("Arithmetic overflow in subtraction of planned cash flows")?;
    trade
        .balance
        .capital_out_market
        .checked_add(change)
        .ok_or_else(|| "Arithmetic overflow in addition of retained trade cash".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mocks::read_transaction_db_mocks::MockDatabase;
    use rust_decimal_macros::dec;

    #[test]
    fn test_calculate_with_empty_trades() {
        let mut database = MockDatabase::new();

        let result =
            TradeCapitalNotAtRisk::calculate(Uuid::new_v4(), &Currency::USD, &mut database);
        assert_eq!(result.unwrap(), dec!(0));
    }

    #[test]
    fn test_calculate_with_one_trade() {
        let mut database = MockDatabase::new();

        database.set_trade(dec!(10), dec!(15), dec!(9), 10);

        let result =
            TradeCapitalNotAtRisk::calculate(Uuid::new_v4(), &Currency::USD, &mut database);
        assert_eq!(result.unwrap(), dec!(90));
    }

    #[test]
    fn test_calculate_with_many_trades() {
        let mut database = MockDatabase::new();

        database.set_trade(dec!(10), dec!(15), dec!(9), 10); // 90
        database.set_trade(dec!(450), dec!(1000), dec!(440), 10); // 4400
        database.set_trade(dec!(323), dec!(1000), dec!(300), 10); // 3000
        database.set_trade(dec!(9), dec!(1000), dec!(6.4), 10); // 64
        database.set_trade(dec!(7.7), dec!(1000), dec!(4.5), 10); // 45

        let result =
            TradeCapitalNotAtRisk::calculate(Uuid::new_v4(), &Currency::USD, &mut database);
        assert_eq!(result.unwrap(), dec!(7599.0));
    }

    #[test]
    fn protected_capital_reports_cash_flow_subtraction_overflow() {
        let mut trade = Trade::default();
        trade.entry.quantity = dec!(1);
        trade.entry.unit_price = Decimal::MAX;
        trade.safety_stop.unit_price = Decimal::MIN;
        assert!(protected_capital(&trade)
            .unwrap_err()
            .to_string()
            .contains("Arithmetic overflow in subtraction"));
    }

    #[test]
    fn test_calculate_reports_per_trade_multiplication_overflow() {
        let mut database = MockDatabase::new();
        database.set_trade(Decimal::MAX, dec!(0), Decimal::MAX, 2);

        let error = TradeCapitalNotAtRisk::calculate(Uuid::new_v4(), &Currency::USD, &mut database)
            .unwrap_err();

        assert!(error
            .to_string()
            .contains("Arithmetic overflow in multiplication"));
    }

    #[test]
    fn test_calculate_reports_total_addition_overflow() {
        let mut database = MockDatabase::new();
        database.set_trade(Decimal::MAX, dec!(0), Decimal::MAX, 1);
        database.set_trade(Decimal::MAX, dec!(0), Decimal::MAX, 1);

        let error = TradeCapitalNotAtRisk::calculate(Uuid::new_v4(), &Currency::USD, &mut database)
            .unwrap_err();

        assert!(error
            .to_string()
            .contains("Arithmetic overflow in addition"));
    }
    #[test]
    fn partial_entries_exits_and_cancelled_remainders_preserve_planned_equity() {
        // (side, entered, exited, entry terminal, retained cash, protected equity)
        for (side, entered, exited, terminal, cash, expected) in [
            (TradeCategory::Long, 0, 0, false, dec!(1000), dec!(900)),
            (TradeCategory::Short, 0, 0, false, dec!(1100), dec!(1000)),
            (TradeCategory::Long, 3, 0, false, dec!(700), dec!(900)),
            (TradeCategory::Short, 3, 0, false, dec!(1400), dec!(1000)),
            (TradeCategory::Long, 3, 0, true, dec!(0), dec!(270)),
            (TradeCategory::Short, 3, 0, true, dec!(1400), dec!(1070)),
            (TradeCategory::Long, 10, 4, true, dec!(480), dec!(1020)),
            (TradeCategory::Long, 10, 4, true, dec!(0), dec!(540)),
            (TradeCategory::Short, 10, 4, true, dec!(1780), dec!(1120)),
        ] {
            let mut trade = Trade {
                category: side,
                ..Trade::default()
            };
            trade.entry.quantity = dec!(10);
            trade.entry.unit_price = dec!(100);
            trade.entry.filled_quantity = Decimal::from(entered);
            trade.entry.status = if terminal {
                OrderStatus::Canceled
            } else {
                OrderStatus::PartiallyFilled
            };
            trade.target.filled_quantity = Decimal::from(exited);
            trade.safety_stop.unit_price = if side == TradeCategory::Long {
                dec!(90)
            } else {
                dec!(110)
            };
            trade.balance.capital_out_market = cash;
            assert_eq!(
                protected_capital(&trade).unwrap(),
                expected,
                "{side} entered {entered}, exited {exited}, terminal {terminal}"
            );
        }
    }
}
