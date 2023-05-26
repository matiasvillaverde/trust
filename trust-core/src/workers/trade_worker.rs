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
        let total_in_market = Decimal::from(trade.entry.quantity) * trade.entry.unit_price.amount;
        database.update_trade_overview_in(trade, total_in_market)?;
        database.update_trade_executed_at(trade)
    }

    pub fn update_trade_target_executed(
        trade: &Trade,
        database: &mut dyn database::Database,
    ) -> Result<Trade, Box<dyn Error>> {
        database.update_trade_overview_in(trade, dec!(0.0))?;
        let total_taxable = dec!(0.0); // TODO: calculate taxes
        let target_order = trade.exit_targets.first().unwrap().order.clone();
        let total_out_market =
            Decimal::from(target_order.quantity) * target_order.unit_price.amount;
        let total_performance =
            total_out_market - total_taxable - trade.overview.total_in_market.amount;
        database.update_trade_overview_out(
            trade,
            total_out_market,
            total_taxable,
            total_performance,
        )?;
        database.update_trade_executed_at(trade)
    }

    pub fn update_trade_stop_executed(
        trade: &Trade,
        database: &mut dyn database::Database,
    ) -> Result<Trade, Box<dyn Error>> {
        database.update_trade_overview_in(trade, dec!(0.0))?;
        let total_taxable = dec!(0.0); // Taxes are not calculated for stop orders
        let stop_order = trade.safety_stop.clone();
        let total_out_market = Decimal::from(stop_order.quantity) * stop_order.unit_price.amount;
        let total_performance = total_out_market - trade.overview.total_in_market.amount;
        database.update_trade_overview_out(
            trade,
            total_out_market,
            total_taxable,
            total_performance,
        )?;
        database.update_trade_executed_at(trade)
    }
}
