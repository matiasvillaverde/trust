use crate::{OrderWorker, TransactionWorker};
use model::{
    Account, AccountOverview, Broker, BrokerLog, DatabaseFactory, DraftTrade, Status, Trade,
    TradeOverview, Transaction,
};
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use std::error::Error;

pub struct TradeAction;

impl TradeAction {
    pub fn update_status(
        trade: &Trade,
        status: Status,
        database: &mut dyn DatabaseFactory,
    ) -> Result<(Trade, Option<Transaction>), Box<dyn Error>> {
        match status {
            Status::Filled if trade.status == Status::Submitted => {
                let (trade, tx) = TradeAction::fill_trade(trade, dec!(0), database)?;
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
                    TradeAction::fill_trade(trade, dec!(0), database)?;
                }

                // We only update the trade target once
                let trade = database.trade_read().read_trade(trade.id)?;
                if trade.status == Status::Filled {
                    // We also update the trade stop loss
                    let (trade, _) = TradeAction::stop_executed(&trade, dec!(0), database)?;
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
                    TradeAction::fill_trade(trade, dec!(0), database)?;
                }

                // We only update the trade target once
                let trade = database.trade_read().read_trade(trade.id)?;
                if trade.status == Status::Filled || trade.status == Status::Canceled {
                    // It can be canceled if the target was updated.
                    // We also update the trade stop loss
                    let (trade, _) = TradeAction::target_executed(&trade, dec!(0), database)?;
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
            database.order_write().as_mut(),
            database.trade_read().as_mut(),
        )?;

        // Record timestamp when the trade was opened
        let trade = database
            .trade_write()
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
            database.order_write().as_mut(),
            database.trade_read().as_mut(),
        )?;

        // 4. Record timestamp when the trade was closed
        let trade = database
            .trade_write()
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
            database.order_write().as_mut(),
            database.trade_read().as_mut(),
        )?;

        // 4. Record timestamp when the trade was closed
        let trade = database
            .trade_write()
            .update_trade_status(Status::ClosedStopLoss, trade)?;

        Ok((trade, tx))
    }

    pub fn create_trade(
        trade: DraftTrade,
        stop_price: Decimal,
        entry_price: Decimal,
        target_price: Decimal,
        database: &mut dyn DatabaseFactory,
    ) -> Result<Trade, Box<dyn std::error::Error>> {
        // 1. Create Stop-loss Order
        let stop = OrderWorker::create_stop(
            trade.trading_vehicle.id,
            trade.quantity,
            stop_price,
            &trade.currency,
            &trade.category,
            database,
        )?;

        // 2. Create Entry Order
        let entry = OrderWorker::create_entry(
            trade.trading_vehicle.id,
            trade.quantity,
            entry_price,
            &trade.currency,
            &trade.category,
            database,
        )?;

        // 3. Create Target Order
        let target = OrderWorker::create_target(
            trade.trading_vehicle.id,
            trade.quantity,
            target_price,
            &trade.currency,
            &trade.category,
            database,
        )?;

        // 4. Create Trade
        let draft = DraftTrade {
            account: trade.account,
            trading_vehicle: trade.trading_vehicle,
            quantity: trade.quantity,
            currency: trade.currency,
            category: trade.category,
        };

        database
            .trade_write()
            .create_trade(draft, &stop, &entry, &target)
    }

    pub fn stop_trade(
        trade: &Trade,
        fee: Decimal,
        database: &mut dyn DatabaseFactory,
    ) -> Result<
        (Transaction, Transaction, TradeOverview, AccountOverview),
        Box<dyn std::error::Error>,
    > {
        let (trade, tx_stop) = TradeAction::stop_executed(trade, fee, database)?;
        let (tx_payment, account_overview, trade_overview) =
            TransactionWorker::transfer_payment_from(&trade, database)?;
        Ok((tx_stop, tx_payment, trade_overview, account_overview))
    }

    pub fn target_acquired(
        trade: &Trade,
        fee: Decimal,
        database: &mut dyn DatabaseFactory,
    ) -> Result<
        (Transaction, Transaction, TradeOverview, AccountOverview),
        Box<dyn std::error::Error>,
    > {
        let (trade, tx_target) = TradeAction::target_executed(trade, fee, database)?;
        let (tx_payment, account_overview, trade_overview) =
            TransactionWorker::transfer_payment_from(&trade, database)?;
        Ok((tx_target, tx_payment, trade_overview, account_overview))
    }

    pub fn cancel_funded_trade(
        trade: &Trade,
        database: &mut dyn DatabaseFactory,
    ) -> Result<(TradeOverview, AccountOverview, Transaction), Box<dyn std::error::Error>> {
        // 1. Verify trade can be canceled
        crate::validators::trade::can_cancel_funded(trade)?;

        // 2. Update Trade Status
        database
            .trade_write()
            .update_trade_status(Status::Canceled, trade)?;

        // 3. Transfer funds back to account
        let (tx, account_o, trade_o) = TransactionWorker::transfer_payment_from(trade, database)?;

        Ok((trade_o, account_o, tx))
    }

    pub fn cancel_submitted_trade(
        trade: &Trade,
        database: &mut dyn DatabaseFactory,
        broker: &mut dyn Broker,
    ) -> Result<(TradeOverview, AccountOverview, Transaction), Box<dyn std::error::Error>> {
        // 1. Verify trade can be canceled
        crate::validators::trade::can_cancel_submitted(trade)?;

        // 2. Cancel trade with broker
        let account = database.account_read().id(trade.account_id)?;
        broker.cancel_trade(trade, &account)?;

        // 3. Update Trade Status
        database
            .trade_write()
            .update_trade_status(Status::Canceled, trade)?;

        // 4. Transfer funds back to account
        let (tx, account_o, trade_o) = TransactionWorker::transfer_payment_from(trade, database)?;

        Ok((trade_o, account_o, tx))
    }

    pub fn modify_stop(
        trade: &Trade,
        account: &Account,
        new_stop_price: Decimal,
        broker: &mut dyn Broker,
        database: &mut dyn DatabaseFactory,
    ) -> Result<(Trade, BrokerLog), Box<dyn std::error::Error>> {
        // 1. Verify trade can be modified
        crate::validators::trade::can_modify_stop(trade, new_stop_price)?;

        // 2. Update Trade on the broker
        let log = broker.modify_stop(trade, account, new_stop_price)?;

        // 3. Modify stop order
        OrderWorker::modify(
            &trade.safety_stop,
            new_stop_price,
            &mut *database.order_write(),
        )?;

        // 4. Refresh Trade
        let trade = database.trade_read().read_trade(trade.id)?;

        Ok((trade, log))
    }

    pub fn modify_target(
        trade: &Trade,
        account: &Account,
        new_price: Decimal,
        broker: &mut dyn Broker,
        database: &mut dyn DatabaseFactory,
    ) -> Result<(Trade, BrokerLog), Box<dyn std::error::Error>> {
        // 1. Verify trade can be modified
        crate::validators::trade::can_modify_target(trade)?;

        // 2. Update Trade on the broker
        let log = broker.modify_target(trade, account, new_price)?;

        // 3. Modify stop order
        OrderWorker::modify(&trade.target, new_price, &mut *database.order_write())?;

        // 4. Refresh Trade
        let trade = database.trade_read().read_trade(trade.id)?;

        Ok((trade, log))
    }
}
