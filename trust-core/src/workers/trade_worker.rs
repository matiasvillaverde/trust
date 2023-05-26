use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use std::error::Error;
use trust_model::{database, Trade};

pub struct TradeWorker;

impl TradeWorker {
    pub fn update_trade_entry_executed(
        trade: &Trade,
        database: &mut dyn database::Database,
    ) -> Result<Trade, Box<dyn Error>> {
        let total_in_market =
            TradeWorker::calculate_total_order(trade.entry.quantity, trade.entry.unit_price.amount);
        database.update_trade_overview_in(trade, total_in_market)?;
        database.update_trade_executed_at(trade)
    }

    pub fn update_trade_target_executed(
        trade: &Trade,
        database: &mut dyn database::Database,
    ) -> Result<Trade, Box<dyn Error>> {
        database.update_trade_overview_in(trade, dec!(0.0))?;
        let total_taxable = TradeWorker::calculate_taxes();
        let target_order = trade.exit_targets.first().unwrap().order.clone();
        let total_performance = TradeWorker::calculate_performance(
            target_order.quantity,
            target_order.unit_price.amount,
            trade.entry.unit_price.amount,
        );
        let total_out_market = TradeWorker::calculate_total_order(
            target_order.quantity,
            target_order.unit_price.amount,
        );
        database.update_trade_overview_out(
            trade,
            total_out_market,
            total_taxable,
            total_performance,
        )?;
        database.update_trade_executed_at(trade)
    }

    fn calculate_performance(
        quantity: u64,
        exit_unit_price: Decimal,
        entry_unit_price: Decimal,
    ) -> Decimal {
        let total_entry = entry_unit_price * Decimal::from(quantity);
        let total_exit = exit_unit_price * Decimal::from(quantity);
        return total_exit - total_entry;
    }

    fn calculate_total_order(quantity: u64, unit_price: Decimal) -> Decimal {
        Decimal::from(quantity) * unit_price
    }

    fn calculate_taxes() -> Decimal {
        dec!(0.0)
    }

    pub fn update_trade_stop_executed(
        trade: &Trade,
        database: &mut dyn database::Database,
    ) -> Result<Trade, Box<dyn Error>> {
        database.update_trade_overview_in(trade, dec!(0.0))?;
        let total_out_market = TradeWorker::calculate_total_order(
            trade.safety_stop.quantity,
            trade.safety_stop.unit_price.amount,
        );
        let total_performance = TradeWorker::calculate_performance(
            trade.safety_stop.quantity,
            trade.safety_stop.unit_price.amount,
            trade.entry.unit_price.amount,
        );
        database.update_trade_overview_out(
            trade,
            total_out_market,
            dec!(0.0), // Taxes are not calculated for stop orders
            total_performance,
        )?;
        database.update_trade_executed_at(trade)
    }
}
