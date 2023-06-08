use crate::{OrderWorker, TransactionWorker};
use rust_decimal::Decimal;
use std::error::Error;
use trust_model::{DatabaseFactory, Trade, Transaction};

pub struct TradeWorker;

impl TradeWorker {
    pub fn fill_trade(
        trade: &Trade,
        fee: Decimal,
        database: &mut dyn DatabaseFactory,
    ) -> Result<Trade, Box<dyn Error>> {
        // Create Transaction to pay for fees
        if fee.is_sign_positive() {
            TransactionWorker::transfer_opening_fee(fee, trade, database)?;
        } else {
            panic!("Fee cannot be negative");
        }

        // Create Transaction to transfer funds to the market
        TransactionWorker::transfer_to_fill_trade(trade, database)?;

        // Record timestamp when the order was opened
        OrderWorker::record_timestamp_entry(
            trade,
            database.write_order_db().as_mut(),
            database.read_trade_db().as_mut(),
        )?;

        // Record timestamp when the trade was opened
        database.write_trade_db().fill_trade(trade)
    }

    pub fn update_trade_target_executed(
        trade: &Trade,
        fee: Decimal,
        database: &mut dyn DatabaseFactory,
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
        OrderWorker::record_timestamp_target(
            trade,
            database.write_order_db().as_mut(),
            database.read_trade_db().as_mut(),
        )?;

        // Record timestamp when the trade was closed
        let trade = database.write_trade_db().stop_trade(trade)?;

        Ok((trade, tx))
    }

    pub fn update_trade_stop_executed(
        trade: &Trade,
        fee: Decimal,
        database: &mut dyn DatabaseFactory,
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
        OrderWorker::record_timestamp_stop(
            trade,
            database.write_order_db().as_mut(),
            database.read_trade_db().as_mut(),
        )?;

        // Record timestamp when the trade was closed
        let trade = database.write_trade_db().stop_trade(trade)?;

        Ok((trade, tx))
    }
}
