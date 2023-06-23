use crate::{OrderWorker, TransactionWorker};
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use std::error::Error;
use model::{DatabaseFactory, Status, Trade, Transaction};

pub struct TradeWorker;

impl TradeWorker {
    pub fn update_status(
        trade: &Trade,
        status: Status,
        database: &mut dyn DatabaseFactory,
    ) -> Result<(Trade, Option<Transaction>), Box<dyn Error>> {
        match status {
            Status::Filled if trade.status == Status::Submitted => {
                let (trade, tx) = TradeWorker::fill_trade(trade, dec!(0), database)?;
                return Ok((trade, Some(tx)));
            }
            Status::Filled if trade.status == Status::Filled => {
                return Ok((trade.clone(), None)); // Nothing to update.
            }
            Status::ClosedStopLoss if trade.status == Status::ClosedStopLoss => {
                return Ok((trade.clone(), None)); // Nothing to update.
            }
            Status::ClosedStopLoss => {
                if trade.status == Status::Submitted {
                    // We also update the trade entry
                    TradeWorker::fill_trade(trade, dec!(0), database)?;
                }

                // We only update the trade target once
                let trade = database.read_trade_db().read_trade(trade.id)?;
                if trade.status == Status::Filled {
                    // We also update the trade stop loss
                    let (trade, _) = TradeWorker::stop_executed(&trade, dec!(0), database)?;
                    let (tx, _, _) = TransactionWorker::transfer_payment_from(&trade, database)?;

                    return Ok((trade, Some(tx)));
                }
            }
            Status::ClosedTarget if trade.status == Status::ClosedTarget => {
                return Ok((trade.clone(), None)); // Nothing to update.
            }
            Status::ClosedTarget => {
                if trade.status == Status::Submitted {
                    // We also update the trade entry
                    TradeWorker::fill_trade(trade, dec!(0), database)?;
                }

                // We only update the trade target once
                let trade = database.read_trade_db().read_trade(trade.id)?;
                if trade.status == Status::Filled || trade.status == Status::Canceled {
                    // It can be canceled if the target was updated.
                    // We also update the trade stop loss
                    let (trade, _) = TradeWorker::target_executed(&trade, dec!(0), database)?;
                    let (tx, _, _) = TransactionWorker::transfer_payment_from(&trade, database)?;

                    return Ok((trade, Some(tx)));
                }
            }
            Status::Submitted if trade.status == Status::Submitted => {
                return Ok((trade.clone(), None));
            }
            _ => {
                return Err(format!("Status can not be updated in trade: {:?}", status).into());
            }
        }
        unimplemented!()
    }

    pub fn fill_trade(
        trade: &Trade,
        fee: Decimal,
        database: &mut dyn DatabaseFactory,
    ) -> Result<(Trade, Transaction), Box<dyn Error>> {
        // Create Transaction to pay for fees
        if fee > dec!(0) {
            TransactionWorker::transfer_opening_fee(fee, trade, database)?;
        }

        // Create Transaction to transfer funds to the market
        let (tx, _) = TransactionWorker::transfer_to_fill_trade(trade, database)?;

        // Record timestamp when the order was opened
        OrderWorker::record_timestamp_filled(
            trade,
            database.write_order_db().as_mut(),
            database.read_trade_db().as_mut(),
        )?;

        // Record timestamp when the trade was opened
        let trade = database
            .write_trade_db()
            .update_trade_status(Status::Filled, trade)?;

        Ok((trade, tx))
    }

    pub fn target_executed(
        trade: &Trade,
        fee: Decimal,
        database: &mut dyn DatabaseFactory,
    ) -> Result<(Trade, Transaction), Box<dyn Error>> {
        // 1. Create Transaction to pay for fees
        if fee > dec!(0) {
            TransactionWorker::transfer_closing_fee(fee, trade, database)?;
        }

        // 2. Create Transaction to transfer funds from the market to the trade
        let (tx, _) = TransactionWorker::transfer_to_close_target(trade, database)?;

        // 3. Record timestamp when the target order was closed
        OrderWorker::record_timestamp_target(
            trade,
            database.write_order_db().as_mut(),
            database.read_trade_db().as_mut(),
        )?;

        // 4. Record timestamp when the trade was closed
        let trade = database
            .write_trade_db()
            .update_trade_status(Status::ClosedTarget, trade)?;

        Ok((trade, tx))
    }

    pub fn stop_executed(
        trade: &Trade,
        fee: Decimal,
        database: &mut dyn DatabaseFactory,
    ) -> Result<(Trade, Transaction), Box<dyn Error>> {
        // 1. Create Transaction to pay for fees
        if fee > dec!(0) {
            TransactionWorker::transfer_closing_fee(fee, trade, database)?;
        }

        // 2. Create Transaction to transfer funds from the market to the trade
        let (tx, _) = TransactionWorker::transfer_to_close_stop(trade, database)?;

        // 3. Record timestamp when the stop order was closed
        OrderWorker::record_timestamp_stop(
            trade,
            database.write_order_db().as_mut(),
            database.read_trade_db().as_mut(),
        )?;

        // 4. Record timestamp when the trade was closed
        let trade = database
            .write_trade_db()
            .update_trade_status(Status::ClosedStopLoss, trade)?;

        Ok((trade, tx))
    }
}
