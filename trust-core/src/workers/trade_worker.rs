use crate::{OrderWorker, TransactionWorker};
use std::error::Error;
use trust_model::{database, Trade};

pub struct TradeWorker;

impl TradeWorker {
    pub fn update_trade_entry_executed(
        trade: &Trade,
        database: &mut dyn database::Database,
    ) -> Result<Trade, Box<dyn Error>> {
        // Create Transaction to transfer funds to the market
        TransactionWorker::transfer_to_open_trade(trade, database)?;

        // Record timestamp when the order was opened
        OrderWorker::record_timestamp_entry(trade, database)
    }

    pub fn update_trade_target_executed(
        trade: &Trade,
        database: &mut dyn database::Database,
    ) -> Result<Trade, Box<dyn Error>> {
        // Create Transaction to transfer funds from the market to the trade
        TransactionWorker::transfer_to_close_target(trade, database)?;

        // Record timestamp when the order was closed
        OrderWorker::record_timestamp_target(trade, database)
    }

    pub fn update_trade_stop_executed(
        trade: &Trade,
        database: &mut dyn database::Database,
    ) -> Result<Trade, Box<dyn Error>> {
        // Create Transaction to transfer funds from the market to the trade
        TransactionWorker::transfer_to_close_stop(trade, database)?;

        // Record timestamp when the order was closed
        OrderWorker::record_timestamp_stop(trade, database)
    }
}
