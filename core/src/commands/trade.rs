use crate::commands;
use chrono::Utc;
use model::{
    Account, AccountBalance, Broker, BrokerLog, DatabaseFactory, DraftTrade, Order, OrderStatus,
    Status, Trade, TradeBalance, Transaction,
};
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use std::collections::HashSet;
use std::error::Error;

type SyncWithBrokerResult = Result<(Status, Vec<Order>, BrokerLog, bool, Trade), Box<dyn Error>>;

fn with_savepoint<T>(
    database: &mut dyn DatabaseFactory,
    name: &str,
    operation: impl FnOnce(&mut dyn DatabaseFactory) -> Result<T, Box<dyn Error>>,
) -> Result<T, Box<dyn Error>> {
    database.begin_savepoint(name)?;
    match operation(database) {
        Ok(value) => {
            database.release_savepoint(name)?;
            Ok(value)
        }
        Err(operation_error) => {
            let rollback_error = database.rollback_to_savepoint(name).err();
            let release_error = database.release_savepoint(name).err();
            if rollback_error.is_none() && release_error.is_none() {
                return Err(operation_error);
            }

            let mut message = format!(
                "operation failed inside savepoint '{name}': {}",
                operation_error
            );
            if let Some(error) = rollback_error {
                message.push_str(&format!("; rollback failed: {error}"));
            }
            if let Some(error) = release_error {
                message.push_str(&format!("; release failed: {error}"));
            }
            Err(message.into())
        }
    }
}

fn update_trade_status_and_projection(
    trade: &Trade,
    status: Status,
    database: &mut dyn DatabaseFactory,
) -> Result<Trade, Box<dyn Error>> {
    let mut updated = database.trade_write().update_trade_status(status, trade)?;
    // Status transitions rely on up-to-date balance values (for example, total_in_trade deltas).
    updated.balance = database.trade_read().read_trade_balance(trade.balance.id)?;
    let _ = commands::balance::apply_account_projection_for_trade_status_transition(
        database, trade, status,
    )?;
    Ok(updated)
}

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
        thesis: trade.thesis,
        sector: trade.sector,
        asset_class: trade.asset_class,
        context: trade.context,
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
            let (trade, tx) = fill_trade_internal(trade, dec!(0), database, false)?;
            return Ok((trade, Some(tx)));
        }
        Status::Filled if trade.status == Status::Filled => {
            return Ok((trade.clone(), None)); // Nothing to update.
        }
        Status::ClosedStopLoss if trade.status == Status::ClosedStopLoss => {
            return Ok((trade.clone(), None)); // Nothing to update.
        }
        Status::ClosedStopLoss => {
            let trade = if trade.status == Status::Submitted {
                // We also update the trade entry.
                let (filled_trade, _) = fill_trade_internal(trade, dec!(0), database, false)?;
                filled_trade
            } else {
                // The incoming trade snapshot is already the latest persisted state.
                trade.clone()
            };
            if trade.status == Status::Filled {
                // We also update the trade stop loss
                let (trade, _) = stop_executed_internal(&trade, dec!(0), database, false)?;
                let (tx, _, _) = commands::transaction::transfer_to_account_from(&trade, database)?;

                return Ok((trade, Some(tx)));
            }
        }
        Status::ClosedTarget if trade.status == Status::ClosedTarget => {
            return Ok((trade.clone(), None)); // Nothing to update.
        }
        Status::ClosedTarget => {
            let trade = if trade.status == Status::Submitted {
                // We also update the trade entry.
                let (filled_trade, _) = fill_trade_internal(trade, dec!(0), database, false)?;
                filled_trade
            } else {
                // The incoming trade snapshot is already the latest persisted state.
                trade.clone()
            };
            if trade.status == Status::Filled || trade.status == Status::Canceled {
                // It can be canceled if the target was updated.
                // We also update the trade stop loss
                let (trade, _) = target_executed_internal(&trade, dec!(0), database, false)?;
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

fn fill_trade_internal(
    trade: &Trade,
    fee: Decimal,
    database: &mut dyn DatabaseFactory,
    record_order_timestamps: bool,
) -> Result<(Trade, Transaction), Box<dyn Error>> {
    // Create Transaction to pay for fees
    if fee > dec!(0) {
        commands::transaction::transfer_opening_fee(fee, trade, database)?;
    }

    // Create Transaction to transfer funds to the market
    let (tx, _) = commands::transaction::transfer_to_fill_trade(trade, database)?;

    if record_order_timestamps {
        // Record timestamp when the order was opened
        commands::order::record_timestamp_filled(trade, database.order_write().as_mut())?;
    }

    // Record timestamp when the trade was opened
    let trade = update_trade_status_and_projection(trade, Status::Filled, database)?;

    Ok((trade, tx))
}

pub fn fill_trade(
    trade: &Trade,
    fee: Decimal,
    database: &mut dyn DatabaseFactory,
) -> Result<(Trade, Transaction), Box<dyn Error>> {
    fill_trade_internal(trade, fee, database, true)
}

fn target_executed_internal(
    trade: &Trade,
    fee: Decimal,
    database: &mut dyn DatabaseFactory,
    record_order_timestamps: bool,
) -> Result<(Trade, Transaction), Box<dyn Error>> {
    // 1. Create Transaction to pay for fees
    if fee > dec!(0) {
        commands::transaction::transfer_closing_fee(fee, trade, database)?;
    }

    // 2. Create Transaction to transfer funds from the market to the trade
    let (tx, _) = commands::transaction::transfer_to_close_target(trade, database)?;

    if record_order_timestamps {
        // 3. Record timestamp when the target order was closed
        commands::order::record_timestamp_target(trade, database.order_write().as_mut())?;
    }

    // 4. Record timestamp when the trade was closed
    let trade = update_trade_status_and_projection(trade, Status::ClosedTarget, database)?;

    Ok((trade, tx))
}

pub fn target_executed(
    trade: &Trade,
    fee: Decimal,
    database: &mut dyn DatabaseFactory,
) -> Result<(Trade, Transaction), Box<dyn Error>> {
    target_executed_internal(trade, fee, database, true)
}

fn stop_executed_internal(
    trade: &Trade,
    fee: Decimal,
    database: &mut dyn DatabaseFactory,
    record_order_timestamps: bool,
) -> Result<(Trade, Transaction), Box<dyn Error>> {
    // 1. Create Transaction to pay for fees
    if fee > dec!(0) {
        commands::transaction::transfer_closing_fee(fee, trade, database)?;
    }

    // 2. Create Transaction to transfer funds from the market to the trade
    let (tx, _) = commands::transaction::transfer_to_close_stop(trade, database)?;

    if record_order_timestamps {
        // 3. Record timestamp when the stop order was closed
        commands::order::record_timestamp_stop(trade, database.order_write().as_mut())?;
    }

    // 4. Record timestamp when the trade was closed
    let trade = update_trade_status_and_projection(trade, Status::ClosedStopLoss, database)?;

    Ok((trade, tx))
}

pub fn stop_executed(
    trade: &Trade,
    fee: Decimal,
    database: &mut dyn DatabaseFactory,
) -> Result<(Trade, Transaction), Box<dyn Error>> {
    stop_executed_internal(trade, fee, database, true)
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
    let _ = update_trade_status_and_projection(trade, Status::Canceled, database)?;

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
    let _ = update_trade_status_and_projection(trade, Status::Canceled, database)?;

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
    let mut funded_trade = update_trade_status_and_projection(trade, Status::Funded, database)?;

    // 3. Create transaction to fund the trade
    let (transaction, account_balance, trade_balance) =
        commands::transaction::transfer_to_fund_trade(trade, database)?;
    // Keep the returned trade snapshot consistent with persisted projections.
    funded_trade.balance = trade_balance.clone();

    // 4. Return data objects
    Ok((funded_trade, transaction, account_balance, trade_balance))
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
    let mut submitted = update_trade_status_and_projection(trade, Status::Submitted, database)?;

    // 5. Update internal orders to submitted and return the updated in-memory trade snapshot.
    let mut order_write = database.order_write();
    submitted.safety_stop = order_write.submit_of(&submitted.safety_stop, order_id.stop)?;
    submitted.entry = order_write.submit_of(&submitted.entry, order_id.entry)?;
    submitted.target = order_write.submit_of(&submitted.target, order_id.target)?;

    Ok((submitted, log))
}

pub fn sync_with_broker(
    trade: &Trade,
    account: &Account,
    database: &mut dyn DatabaseFactory,
    broker: &mut dyn Broker,
) -> SyncWithBrokerResult {
    // Best-effort execution reconciliation window.
    // This is intentionally outside the DB savepoint because it may involve broker I/O.
    let after = database
        .execution_read()
        .latest_trade_execution_at(trade.id)?;
    let after =
        after.map(|t| chrono::DateTime::<chrono::Utc>::from_naive_utc_and_offset(t, chrono::Utc));
    let executions = match broker.fetch_executions(trade, account, after) {
        Ok(executions) => executions,
        Err(e) => {
            // Execution ingestion must never block core sync reliability.
            // We still persist the normal trade/order snapshot. The reconciliation can be retried.
            eprintln!(
                "Execution reconciliation failed for trade {}: {e}",
                trade.id
            );
            vec![]
        }
    };
    // 1. Sync Trade with Broker
    let (status, orders, log) = broker.sync_trade(trade, account)?;

    // 2. Persist the whole sync lifecycle atomically.
    let (transitioned_to_closed, persisted_trade) =
        with_savepoint(database, "sync_trade_lifecycle", |database| {
            // 2.1 Save broker log.
            database.log_write().create_log(log.log.as_str(), trade)?;

            // 2.2 Resolve broker updates against the latest persisted trade snapshot.
            // Perf: `read_trade` is expensive (joins / multiple reads). In the common case the caller
            // passes the latest persisted snapshot; validate that via a lightweight status read and
            // only fall back to `read_trade` when needed.
            let persisted_status = database.trade_read().read_trade_status(trade.id)?;
            let current_trade = if persisted_status == trade.status {
                trade.clone()
            } else {
                database.trade_read().read_trade(trade.id)?
            };
            let was_closed = persisted_status == Status::ClosedTarget
                || persisted_status == Status::ClosedStopLoss;
            let is_closed = status == Status::ClosedTarget || status == Status::ClosedStopLoss;
            let transitioned_to_closed = is_closed && !was_closed;
            let resolved = resolve_orders_for_sync(&current_trade, &orders)?;

            // 2.3 Validate payload before mutating DB state.
            validate_sync_payload(&current_trade, account, status, &resolved)?;

            // 2.4 Persist order changes only when broker state actually changed.
            {
                let mut order_write = database.order_write();
                if should_persist_order_update(&current_trade.entry, &resolved.entry) {
                    order_write.update(&resolved.entry)?;
                }
                if should_persist_order_update(&current_trade.target, &resolved.target) {
                    order_write.update(&resolved.target)?;
                }
                if should_persist_order_update(&current_trade.safety_stop, &resolved.stop) {
                    order_write.update(&resolved.stop)?;
                }
            }

            // 2.5 Update trade status from the latest persisted trade snapshot.
            let mut trade_with_synced_orders = current_trade;
            trade_with_synced_orders.entry = resolved.entry;
            trade_with_synced_orders.target = resolved.target;
            trade_with_synced_orders.safety_stop = resolved.stop;
            let (trade, _) = update_status(&trade_with_synced_orders, status, database)?;
            // 2.6 Persist executions (fills) for execution-level accounting, idempotently.
            //
            // Important: we only attribute executions to this trade when the broker order ID matches
            // one of the trade orders. This prevents cross-trade contamination on the same symbol.
            persist_executions_for_trade(database, &trade, &executions)?;

            // 2.6 Ensure sibling exit orders are consistently closed in terminal states.
            reconcile_sibling_exit_orders(&trade, status, database)?;
            Ok((transitioned_to_closed, trade))
        })?;

    Ok((status, orders, log, transitioned_to_closed, persisted_trade))
}

fn persist_executions_for_trade(
    database: &mut dyn DatabaseFactory,
    trade: &Trade,
    executions: &[model::Execution],
) -> Result<(), Box<dyn Error>> {
    let trade_symbol = trade.trading_vehicle.symbol.as_str();
    let entry_broker_id = trade.entry.broker_order_id;
    let target_broker_id = trade.target.broker_order_id;
    let stop_broker_id = trade.safety_stop.broker_order_id;

    let mut write = database.execution_write();
    for exec in executions {
        // Strict validation: never store garbage amounts.
        if exec.qty <= rust_decimal_macros::dec!(0) {
            return Err(format!(
                "invalid execution qty for broker_execution_id {}: {}",
                exec.broker_execution_id, exec.qty
            )
            .into());
        }
        if exec.price <= rust_decimal_macros::dec!(0) {
            return Err(format!(
                "invalid execution price for broker_execution_id {}: {}",
                exec.broker_execution_id, exec.price
            )
            .into());
        }
        if exec.symbol != trade_symbol {
            // Broker impl should have filtered, but keep this as a hard safety check.
            continue;
        }

        let Some(broker_order_id) = exec.broker_order_id else {
            continue;
        };

        let order_id = if Some(broker_order_id) == entry_broker_id {
            Some(trade.entry.id)
        } else if Some(broker_order_id) == target_broker_id {
            Some(trade.target.id)
        } else if Some(broker_order_id) == stop_broker_id {
            Some(trade.safety_stop.id)
        } else {
            // Execution on same symbol but not part of this trade.
            continue;
        };

        let mut attributed = exec.clone();
        attributed.trade_id = Some(trade.id);
        attributed.order_id = order_id;

        let _ = write.upsert_execution(&attributed)?;
    }

    Ok(())
}

fn validate_sync_payload(
    trade: &Trade,
    account: &Account,
    status: Status,
    resolved: &ResolvedSyncOrders,
) -> Result<(), Box<dyn Error>> {
    if trade.account_id != account.id {
        return Err(format!(
            "sync account mismatch: trade account {} does not match provided account {}",
            trade.account_id, account.id
        )
        .into());
    }

    validate_sync_transition(trade, status)?;

    match status {
        Status::Submitted => {
            let has_fill_like = [
                resolved.entry.status,
                resolved.target.status,
                resolved.stop.status,
            ]
            .iter()
            .any(|s| matches!(s, OrderStatus::Filled | OrderStatus::PartiallyFilled));
            if has_fill_like {
                return Err(
                    "inconsistent sync payload: submitted trade contains filled order state".into(),
                );
            }
        }
        Status::Filled => {
            ensure_order_filled(&resolved.entry, "entry")?;
        }
        Status::ClosedTarget => {
            ensure_order_filled(&resolved.entry, "entry")?;
            ensure_order_filled(&resolved.target, "target")?;
        }
        Status::ClosedStopLoss => {
            ensure_order_filled(&resolved.entry, "entry")?;
            ensure_order_filled(&resolved.stop, "stop")?;
        }
        _ => {}
    }

    Ok(())
}

fn should_persist_order_update(current: &Order, resolved: &Order) -> bool {
    current.broker_order_id != resolved.broker_order_id
        || current.status != resolved.status
        || current.filled_quantity != resolved.filled_quantity
        || current.average_filled_price != resolved.average_filled_price
        || current.submitted_at != resolved.submitted_at
        || current.filled_at != resolved.filled_at
        || current.expired_at != resolved.expired_at
        || current.cancelled_at != resolved.cancelled_at
        || current.closed_at != resolved.closed_at
        || current.category != resolved.category
}

fn validate_sync_transition(trade: &Trade, status: Status) -> Result<(), Box<dyn Error>> {
    match status {
        Status::Filled if trade.status == Status::Submitted || trade.status == Status::Filled => {
            Ok(())
        }
        Status::ClosedStopLoss
            if trade.status == Status::Submitted
                || trade.status == Status::Filled
                || trade.status == Status::ClosedStopLoss =>
        {
            Ok(())
        }
        Status::ClosedTarget
            if trade.status == Status::Submitted
                || trade.status == Status::Filled
                || trade.status == Status::Canceled
                || trade.status == Status::ClosedTarget =>
        {
            Ok(())
        }
        Status::Submitted if trade.status == Status::Submitted => Ok(()),
        _ => Err(format!(
            "invalid sync transition: trade {} is {:?}, broker reported {:?}",
            trade.id, trade.status, status
        )
        .into()),
    }
}

struct ResolvedSyncOrders {
    entry: Order,
    target: Order,
    stop: Order,
}

fn merge_sync_order_state(base: &Order, update: &Order) -> Order {
    let mut merged = base.clone();
    merged.broker_order_id = update.broker_order_id;
    merged.status = update.status;
    merged.filled_quantity = update.filled_quantity;
    merged.average_filled_price = update.average_filled_price;
    merged.submitted_at = update.submitted_at;
    merged.filled_at = update.filled_at;
    merged.expired_at = update.expired_at;
    merged.cancelled_at = update.cancelled_at;
    merged.closed_at = update.closed_at;
    merged
}

fn resolve_orders_for_sync(
    trade: &Trade,
    orders: &[Order],
) -> Result<ResolvedSyncOrders, Box<dyn Error>> {
    let mut entry = trade.entry.clone();
    let mut target = trade.target.clone();
    let mut stop = trade.safety_stop.clone();
    let mut seen_ids = HashSet::new();

    for order in orders {
        if !seen_ids.insert(order.id) {
            return Err(format!(
                "duplicate order update in sync payload for order id {}",
                order.id
            )
            .into());
        }

        if order.id == entry.id {
            entry = merge_sync_order_state(&entry, order);
        } else if order.id == target.id {
            target = merge_sync_order_state(&target, order);
        } else if order.id == stop.id {
            stop = merge_sync_order_state(&stop, order);
        } else {
            return Err(format!(
                "order id {} not found in trade {} order set during sync",
                order.id, trade.id
            )
            .into());
        }
    }

    Ok(ResolvedSyncOrders {
        entry,
        target,
        stop,
    })
}

fn ensure_order_filled(order: &Order, role: &str) -> Result<(), Box<dyn Error>> {
    if order.status != OrderStatus::Filled {
        return Err(format!(
            "inconsistent sync payload: expected {role} order {} to be filled, found {:?}",
            order.id, order.status
        )
        .into());
    }

    if order.average_filled_price.is_none() {
        return Err(format!(
            "inconsistent sync payload: filled {role} order {} has no average filled price",
            order.id
        )
        .into());
    }

    Ok(())
}

fn reconcile_sibling_exit_orders(
    trade: &Trade,
    status: Status,
    database: &mut dyn DatabaseFactory,
) -> Result<(), Box<dyn Error>> {
    let now = Utc::now().naive_utc();
    let mut order_write = database.order_write();

    match status {
        Status::ClosedTarget => {
            if !is_terminal_order_status(trade.safety_stop.status) {
                let mut stop = trade.safety_stop.clone();
                stop.status = OrderStatus::Canceled;
                if stop.cancelled_at.is_none() {
                    stop.cancelled_at = Some(now);
                }
                if stop.closed_at.is_none() {
                    stop.closed_at = Some(now);
                }
                order_write.update(&stop)?;
            }
        }
        Status::ClosedStopLoss => {
            if !is_terminal_order_status(trade.target.status) {
                let mut target = trade.target.clone();
                target.status = OrderStatus::Canceled;
                if target.cancelled_at.is_none() {
                    target.cancelled_at = Some(now);
                }
                if target.closed_at.is_none() {
                    target.closed_at = Some(now);
                }
                order_write.update(&target)?;
            }
        }
        _ => {}
    }

    Ok(())
}

fn is_terminal_order_status(status: OrderStatus) -> bool {
    matches!(
        status,
        OrderStatus::Filled | OrderStatus::Canceled | OrderStatus::Expired | OrderStatus::Rejected
    )
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
    {
        let mut order_write = database.order_write();
        order_write.update(&target_order)?;
    }

    // 5. Update Trade Status
    let _ = update_trade_status_and_projection(trade, Status::Canceled, database)?;

    // 6. Cancel Stop-loss Order
    let mut stop_order = trade.safety_stop.clone();
    stop_order.status = OrderStatus::Canceled;
    {
        let mut order_write = database.order_write();
        order_write.update(&stop_order)?;
    }

    Ok((trade.balance.clone(), log))
}
