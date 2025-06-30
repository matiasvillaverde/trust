use crate::commands;
use model::{
    Account, AccountBalance, Broker, BrokerLog, DatabaseFactory, DraftTrade, Order, OrderStatus,
    Status, Trade, TradeBalance, Transaction,
};
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use std::error::Error;

pub fn create_trade(
    trade: DraftTrade,
    stop_price: Decimal,
    entry_price: Decimal,
    target_price: Decimal,
    database: &mut dyn DatabaseFactory,
) -> Result<Trade, Box<dyn std::error::Error>> {
    // 1. Create Stop-loss Order
    let stop = commands::order::create_stop(
        trade.trading_vehicle.id,
        trade.quantity,
        stop_price,
        &trade.currency,
        &trade.category,
        database,
    )?;

    // 2. Create Entry Order
    let entry = commands::order::create_entry(
        trade.trading_vehicle.id,
        trade.quantity,
        entry_price,
        &trade.currency,
        &trade.category,
        database,
    )?;

    // 3. Create Target Order
    let target = commands::order::create_target(
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

pub fn update_status(
    trade: &Trade,
    status: Status,
    database: &mut dyn DatabaseFactory,
) -> Result<(Trade, Option<Transaction>), Box<dyn Error>> {
    match status {
        Status::Filled if trade.status == Status::Submitted => {
            let (trade, tx) = fill_trade(trade, dec!(0), database)?;
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
                fill_trade(trade, dec!(0), database)?;
            }

            // We only update the trade target once
            let trade = database.trade_read().read_trade(trade.id)?;
            if trade.status == Status::Filled {
                // We also update the trade stop loss
                let (trade, _) = stop_executed(&trade, dec!(0), database)?;
                let (tx, _, _) = commands::transaction::transfer_to_account_from(&trade, database)?;

                return Ok((trade, Some(tx)));
            }
        }
        Status::ClosedTarget if trade.status == Status::ClosedTarget => {
            return Ok((trade.clone(), None)); // Nothing to update.
        }
        Status::ClosedTarget => {
            if trade.status == Status::Submitted {
                // We also update the trade entry
                fill_trade(trade, dec!(0), database)?;
            }

            // We only update the trade target once
            let trade = database.trade_read().read_trade(trade.id)?;
            if trade.status == Status::Filled || trade.status == Status::Canceled {
                // It can be canceled if the target was updated.
                // We also update the trade stop loss
                let (trade, _) = target_executed(&trade, dec!(0), database)?;
                let (tx, _, _) = commands::transaction::transfer_to_account_from(&trade, database)?;

                return Ok((trade, Some(tx)));
            }
        }
        Status::Submitted if trade.status == Status::Submitted => {
            return Ok((trade.clone(), None));
        }
        _ => {
            return Err(format!("Status can not be updated in trade: {status:?}").into());
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
        commands::transaction::transfer_opening_fee(fee, trade, database)?;
    }

    // Create Transaction to transfer funds to the market
    let (tx, _) = commands::transaction::transfer_to_fill_trade(trade, database)?;

    // Record timestamp when the order was opened
    commands::order::record_timestamp_filled(
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
        commands::transaction::transfer_closing_fee(fee, trade, database)?;
    }

    // 2. Create Transaction to transfer funds from the market to the trade
    let (tx, _) = commands::transaction::transfer_to_close_target(trade, database)?;

    // 3. Record timestamp when the target order was closed
    commands::order::record_timestamp_target(
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
        commands::transaction::transfer_closing_fee(fee, trade, database)?;
    }

    // 2. Create Transaction to transfer funds from the market to the trade
    let (tx, _) = commands::transaction::transfer_to_close_stop(trade, database)?;

    // 3. Record timestamp when the stop order was closed
    commands::order::record_timestamp_stop(
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

pub fn stop_acquired(
    trade: &Trade,
    fee: Decimal,
    database: &mut dyn DatabaseFactory,
) -> Result<(Transaction, Transaction, TradeBalance, AccountBalance), Box<dyn std::error::Error>> {
    let (trade, tx_stop) = stop_executed(trade, fee, database)?;
    let (tx_payment, account_balance, trade_balance) =
        commands::transaction::transfer_to_account_from(&trade, database)?;
    Ok((tx_stop, tx_payment, trade_balance, account_balance))
}

pub fn target_acquired(
    trade: &Trade,
    fee: Decimal,
    database: &mut dyn DatabaseFactory,
) -> Result<(Transaction, Transaction, TradeBalance, AccountBalance), Box<dyn std::error::Error>> {
    let (trade, tx_target) = target_executed(trade, fee, database)?;
    let (tx_payment, account_balance, trade_balance) =
        commands::transaction::transfer_to_account_from(&trade, database)?;
    Ok((tx_target, tx_payment, trade_balance, account_balance))
}

pub fn cancel_funded(
    trade: &Trade,
    database: &mut dyn DatabaseFactory,
) -> Result<(TradeBalance, AccountBalance, Transaction), Box<dyn std::error::Error>> {
    // 1. Verify trade can be canceled
    crate::validators::trade::can_cancel_funded(trade)?;

    // 2. Update Trade Status
    database
        .trade_write()
        .update_trade_status(Status::Canceled, trade)?;

    // 3. Transfer funds back to account
    let (tx, account_o, trade_o) =
        commands::transaction::transfer_to_account_from(trade, database)?;

    Ok((trade_o, account_o, tx))
}

pub fn cancel_submitted(
    trade: &Trade,
    database: &mut dyn DatabaseFactory,
    broker: &mut dyn Broker,
) -> Result<(TradeBalance, AccountBalance, Transaction), Box<dyn std::error::Error>> {
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
    let (tx, account_o, trade_o) =
        commands::transaction::transfer_to_account_from(trade, database)?;

    Ok((trade_o, account_o, tx))
}

pub fn modify_stop(
    trade: &Trade,
    account: &Account,
    new_stop_price: Decimal,
    broker: &mut dyn Broker,
    database: &mut dyn DatabaseFactory,
) -> Result<Trade, Box<dyn std::error::Error>> {
    // 1. Verify trade can be modified
    crate::validators::trade::can_modify_stop(trade, new_stop_price)?;

    // 2. Update Trade on the broker
    let new_broker_id = broker.modify_stop(trade, account, new_stop_price)?;

    // 3. Modify stop order
    commands::order::modify(
        &trade.safety_stop,
        new_stop_price,
        new_broker_id,
        &mut *database.order_write(),
    )?;

    // 4. Refresh Trade
    let trade = database.trade_read().read_trade(trade.id)?;

    Ok(trade)
}

pub fn modify_target(
    trade: &Trade,
    account: &Account,
    new_price: Decimal,
    broker: &mut dyn Broker,
    database: &mut dyn DatabaseFactory,
) -> Result<Trade, Box<dyn std::error::Error>> {
    // 1. Verify trade can be modified
    crate::validators::trade::can_modify_target(trade)?;

    // 2. Update Trade on the broker
    let new_broker_id = broker.modify_target(trade, account, new_price)?;

    // 3. Modify stop order
    commands::order::modify(
        &trade.target,
        new_price,
        new_broker_id,
        &mut *database.order_write(),
    )?;

    // 4. Refresh Trade
    let trade = database.trade_read().read_trade(trade.id)?;

    Ok(trade)
}

pub fn fund(
    trade: &Trade,
    database: &mut dyn DatabaseFactory,
) -> Result<(Trade, Transaction, AccountBalance, TradeBalance), Box<dyn std::error::Error>> {
    // 1. Validate that trade can be funded
    crate::validators::funding::can_fund(trade, database)?;

    // 2. Update trade status to funded
    database
        .trade_write()
        .update_trade_status(Status::Funded, trade)?;

    // 3. Create transaction to fund the trade
    let (transaction, account_balance, trade_balance) =
        commands::transaction::transfer_to_fund_trade(trade, database)?;

    // 4. Return data objects
    Ok((trade.clone(), transaction, account_balance, trade_balance))
}

pub fn submit(
    trade: &Trade,
    database: &mut dyn DatabaseFactory,
    broker: &mut dyn Broker,
) -> Result<(Trade, BrokerLog), Box<dyn std::error::Error>> {
    // 1. Validate that Trade can be submitted
    crate::validators::trade::can_submit(trade)?;

    // 2. Submit trade to broker
    let account = database.account_read().id(trade.account_id)?;
    let (log, order_id) = broker.submit_trade(trade, &account)?;

    // 3. Save log in the DB
    database.log_write().create_log(log.log.as_str(), trade)?;

    // 4. Update Trade status to submitted
    let trade = database
        .trade_write()
        .update_trade_status(Status::Submitted, trade)?;

    // 5. Update internal orders orders to submitted
    database
        .order_write()
        .submit_of(&trade.safety_stop, order_id.stop)?;
    database
        .order_write()
        .submit_of(&trade.entry, order_id.entry)?;
    database
        .order_write()
        .submit_of(&trade.target, order_id.target)?;

    // 6. Read Trade with updated values
    let trade = database.trade_read().read_trade(trade.id)?;

    // 7. Return Trade and Log
    Ok((trade, log))
}

pub fn sync_with_broker(
    trade: &Trade,
    account: &Account,
    database: &mut dyn DatabaseFactory,
    broker: &mut dyn Broker,
) -> Result<(Status, Vec<Order>, BrokerLog), Box<dyn std::error::Error>> {
    // 1. Sync Trade with Broker
    let (status, orders, log) = broker.sync_trade(trade, account)?;

    // 2. Save log in the DB
    database.log_write().create_log(log.log.as_str(), trade)?;

    // 3. Update Orders
    for order in orders.clone() {
        commands::order::update_order(&order, database)?;
    }

    // 4. Update Trade Status
    let trade = database.trade_read().read_trade(trade.id)?; // We need to read the trade again to get the updated orders
    update_status(&trade, status, database)?;

    // 5. Update Account Overview
    commands::balance::calculate_account(database, account, &trade.currency)?;

    Ok((status, orders, log))
}

pub fn close(
    trade: &Trade,
    database: &mut dyn DatabaseFactory,
    broker: &mut dyn Broker,
) -> Result<(TradeBalance, BrokerLog), Box<dyn std::error::Error>> {
    // 1. Verify trade can be closed
    crate::validators::trade::can_close(trade)?;

    // 2. Submit a market order to close the trade
    let account = database.account_read().id(trade.account_id)?;
    let (target_order, log) = broker.close_trade(trade, &account)?;

    // 3. Save log in the database
    database.log_write().create_log(log.log.as_str(), trade)?;

    // 4. Update Order Target with the filled price and new ID
    commands::order::update_order(&target_order, database)?;

    // 5. Update Trade Status
    database
        .trade_write()
        .update_trade_status(Status::Canceled, trade)?;

    // 6. Cancel Stop-loss Order
    let mut stop_order = trade.safety_stop.clone();
    stop_order.status = OrderStatus::Canceled;
    database.order_write().update(&stop_order)?;

    Ok((trade.balance.clone(), log))
}
