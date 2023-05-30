use crate::{OrderWorker, TransactionWorker};
use rust_decimal::Decimal;
use std::error::Error;
use trust_model::{database, Trade, Transaction};

pub struct TradeWorker;

impl TradeWorker {
    pub fn open_trade(
        trade: &Trade,
        fee: Decimal,
        database: &mut dyn database::Database,
    ) -> Result<Trade, Box<dyn Error>> {
        // Create Transaction to pay for fees
        if fee.is_sign_positive() {
            TransactionWorker::transfer_opening_fee(fee, trade, database)?;
        } else {
            panic!("Fee cannot be negative");
        }

        // Create Transaction to transfer funds to the market
        TransactionWorker::transfer_to_open_trade(trade, database)?;

        // Record timestamp when the order was opened
        OrderWorker::record_timestamp_entry(trade, database)?;

        // Record timestamp when the trade was opened
        database.update_trade_opened_at(trade)
    }

    pub fn update_trade_target_executed(
        trade: &Trade,
        fee: Decimal,
        database: &mut dyn database::Database,
    ) -> Result<(Trade, Transaction), Box<dyn Error>> {
        // Create Transaction to pay for fees
        if fee.is_sign_positive() {
            TransactionWorker::transfer_closing_fee(fee, trade, database)?;
        } else {
            panic!("Fee cannot be negative");
        }

        // Create Transaction to transfer funds from the market to the trade
        let (tx, _) = TransactionWorker::transfer_to_close_target(trade, database)?;

        // Record timestamp when the order was closed
        OrderWorker::record_timestamp_target(trade, database)?;

        // Record timestamp when the trade was closed
        let trade = database.update_trade_closed_at(trade)?;

        Ok((trade, tx))
    }

    pub fn update_trade_stop_executed(
        trade: &Trade,
        fee: Decimal,
        database: &mut dyn database::Database,
    ) -> Result<(Trade, Transaction), Box<dyn Error>> {
        // Create Transaction to pay for fees
        if fee.is_sign_positive() {
            TransactionWorker::transfer_closing_fee(fee, trade, database)?;
        } else {
            panic!("Fee cannot be negative");
        }

        // Create Transaction to transfer funds from the market to the trade
        let (tx, _) = TransactionWorker::transfer_to_close_stop(trade, database)?;

        // Record timestamp when the order was closed
        OrderWorker::record_timestamp_stop(trade, database)?;

        // Record timestamp when the trade was closed
        let trade = database.update_trade_closed_at(trade)?;

        Ok((trade, tx))
    }
}
