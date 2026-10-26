#![allow(clippy::items_after_test_module)]

use crate::audit::{self, AuditRecord};
use crate::commands;
use chrono::Utc;
use model::{
    Account, AccountBalance, Broker, BrokerError, BrokerKind, BrokerLog, DatabaseFactory,
    DraftTrade, Execution, ExecutionSide, ExecutionSource, FeeActivity, Order, OrderCategory,
    OrderStatus, Status, Trade, TradeBalance, TradeCategory, TradingVehicleCategory, Transaction,
};
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use sha2::Digest;
use std::collections::HashSet;
use std::error::Error;
use uuid::Uuid;

type SyncWithBrokerResult = Result<(Status, Vec<Order>, BrokerLog, bool, Trade), Box<dyn Error>>;

use super::atomic::with_savepoint;

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

fn latest_trade_snapshot(
    trade: &Trade,
    database: &mut dyn DatabaseFactory,
) -> Result<Trade, Box<dyn Error>> {
    database.trade_read().read_trade(trade.id)
}

pub fn create_trade(
    trade: DraftTrade,
    stop_price: Decimal,
    entry_price: Decimal,
    target_price: Decimal,
    database: &mut dyn DatabaseFactory,
) -> Result<Trade, Box<dyn std::error::Error>> {
    create_trade_with_safety_order_category(
        trade,
        stop_price,
        entry_price,
        target_price,
        OrderCategory::Stop,
        database,
    )
}

pub fn create_trade_with_safety_order_category(
    trade: DraftTrade,
    stop_price: Decimal,
    entry_price: Decimal,
    target_price: Decimal,
    safety_order_category: OrderCategory,
    database: &mut dyn DatabaseFactory,
) -> Result<Trade, Box<dyn std::error::Error>> {
    with_savepoint(
        database,
        "create_trade_with_safety_order_category",
        |database| {
            if trade.quantity <= Decimal::ZERO {
                return Err(format!(
                    "Trade quantity must be greater than zero, got {}",
                    trade.quantity
                )
                .into());
            }
            validate_broker_asset_support(&trade)?;

            // 1. Create Stop-loss Order
            let stop = match safety_order_category {
                OrderCategory::Stop => commands::order::create_stop(
                    trade.trading_vehicle.id,
                    trade.quantity,
                    stop_price,
                    &trade.currency,
                    &trade.category,
                    database,
                )?,
                OrderCategory::StopLimit => commands::order::create_stop_with_category(
                    trade.trading_vehicle.id,
                    trade.quantity,
                    stop_price,
                    &trade.currency,
                    &trade.category,
                    &safety_order_category,
                    database,
                )?,
                _ => {
                    return Err(format!(
                "Safety order category must be stop or stop_limit, got {safety_order_category}"
            )
                    .into());
                }
            };

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
        },
    )
}

fn validate_broker_asset_support(trade: &DraftTrade) -> Result<(), Box<dyn std::error::Error>> {
    match trade.trading_vehicle.category {
        TradingVehicleCategory::Bond if vehicle_broker_is(&trade.trading_vehicle, "alpaca") => {
            Err(BrokerError::unsupported_asset_class(
                BrokerKind::Alpaca,
                TradingVehicleCategory::Bond,
                "Bonds are not supported by Alpaca. Use IBKR for bond trading.",
            )
            .into())
        }
        TradingVehicleCategory::Fiat if vehicle_broker_is(&trade.trading_vehicle, "alpaca") => {
            Err(BrokerError::unsupported_asset_class(
                BrokerKind::Alpaca,
                TradingVehicleCategory::Fiat,
                "Fiat spot trading is not supported by Alpaca trade creation.",
            )
            .into())
        }
        _ => Ok(()),
    }
}

fn vehicle_broker_is(vehicle: &model::TradingVehicle, broker: &str) -> bool {
    vehicle.broker.trim().eq_ignore_ascii_case(broker)
}

fn record_manual_execution(
    order: &Order,
    database: &mut dyn DatabaseFactory,
) -> Result<(), Box<dyn Error>> {
    let mut filled = order.clone();
    filled.status = OrderStatus::Filled;
    filled.filled_quantity = filled.quantity;
    filled.filled_at = Some(Utc::now().naive_utc());
    database.order_write().update(&filled)?;
    Ok(())
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
        record_manual_execution(&trade.entry, database)?;
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
    with_savepoint(database, "fill_trade", |database| {
        let mut current = latest_trade_snapshot(trade, database)?;
        if !(matches!(current.status, Status::Funded | Status::Submitted)) {
            return Err("Invalid persisted trade status for manual execution".into());
        }
        if fee < Decimal::ZERO {
            return Err("Execution fee cannot be negative".into());
        }
        // Only the explicit execution price is accepted from the caller snapshot.
        current.entry.average_filled_price = trade.entry.average_filled_price;
        let trade = &current;

        fill_trade_internal(trade, fee, database, true)
    })
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
        record_manual_execution(&trade.target, database)?;
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
    with_savepoint(database, "target_executed", |database| {
        let mut current = latest_trade_snapshot(trade, database)?;
        if !(current.status == Status::Filled) {
            return Err("Invalid persisted trade status for manual execution".into());
        }
        if fee < Decimal::ZERO {
            return Err("Execution fee cannot be negative".into());
        }
        // Only the explicit execution price is accepted from the caller snapshot.
        current.target.average_filled_price = trade.target.average_filled_price;
        let trade = &current;

        target_executed_internal(trade, fee, database, true)
    })
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
        record_manual_execution(&trade.safety_stop, database)?;
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
    with_savepoint(database, "stop_executed", |database| {
        let mut current = latest_trade_snapshot(trade, database)?;
        if !(current.status == Status::Filled) {
            return Err("Invalid persisted trade status for manual execution".into());
        }
        if fee < Decimal::ZERO {
            return Err("Execution fee cannot be negative".into());
        }
        // Only the explicit execution price is accepted from the caller snapshot.
        current.safety_stop.average_filled_price = trade.safety_stop.average_filled_price;
        let trade = &current;

        stop_executed_internal(trade, fee, database, true)
    })
}

pub fn stop_acquired(
    trade: &Trade,
    fee: Decimal,
    database: &mut dyn DatabaseFactory,
) -> Result<(Transaction, Transaction, TradeBalance, AccountBalance), Box<dyn std::error::Error>> {
    with_savepoint(database, "stop_acquired", |database| {
        let (trade, tx_stop) = stop_executed(trade, fee, database)?;
        let (tx_payment, account_balance, trade_balance) =
            commands::transaction::transfer_to_account_from(&trade, database)?;
        Ok((tx_stop, tx_payment, trade_balance, account_balance))
    })
}

pub fn target_acquired(
    trade: &Trade,
    fee: Decimal,
    database: &mut dyn DatabaseFactory,
) -> Result<(Transaction, Transaction, TradeBalance, AccountBalance), Box<dyn std::error::Error>> {
    with_savepoint(database, "target_acquired", |database| {
        let (trade, tx_target) = target_executed(trade, fee, database)?;
        let (tx_payment, account_balance, trade_balance) =
            commands::transaction::transfer_to_account_from(&trade, database)?;
        Ok((tx_target, tx_payment, trade_balance, account_balance))
    })
}

pub fn cancel_funded(
    trade: &Trade,
    database: &mut dyn DatabaseFactory,
) -> Result<(TradeBalance, AccountBalance, Transaction), Box<dyn std::error::Error>> {
    with_savepoint(database, "cancel_funded", |database| {
        // 1. Load latest persisted trade snapshot to prevent stale-state replay.
        let current_trade = latest_trade_snapshot(trade, database)?;

        // 2. Verify trade can be canceled
        crate::validators::trade::can_cancel_funded(&current_trade)?;

        // 3. Update Trade Status
        let _ = update_trade_status_and_projection(&current_trade, Status::Canceled, database)?;

        // 4. Transfer funds back to account
        let (tx, account_o, trade_o) =
            commands::transaction::transfer_to_account_from(&current_trade, database)?;

        Ok((trade_o, account_o, tx))
    })
}

pub fn cancel_submitted(
    trade: &Trade,
    database: &mut dyn DatabaseFactory,
    broker: &mut dyn Broker,
) -> Result<(TradeBalance, AccountBalance, Transaction), Box<dyn std::error::Error>> {
    // 1. Load latest persisted trade snapshot to prevent stale-state replay.
    let current_trade = latest_trade_snapshot(trade, database)?;

    // 2. Verify trade can be canceled
    crate::validators::trade::can_cancel_submitted(&current_trade)?;
    let operation_id = Uuid::new_v4();
    let broker_name = database
        .account_read()
        .id(current_trade.account_id)?
        .broker_kind
        .as_str()
        .to_string();
    let attempt_details =
        serde_json::json!({"capital_release":"blocked_until_zero_fill_confirmation"});
    audit::record(
        database,
        &current_trade,
        &broker_name,
        AuditRecord {
            operation_id,
            event_type: "trade.cancel",
            outcome: "attempted",
            prior_status: current_trade.status,
            resulting_status: current_trade.status,
            details: &attempt_details,
        },
    )?;

    // 3. Cancel trade with broker
    let account = database.account_read().id(current_trade.account_id)?;
    if let Err(error) = broker.cancel_trade(&current_trade, &account) {
        let outcome = if error
            .downcast_ref::<BrokerError>()
            .is_some_and(BrokerError::is_retry_safe_mutation)
        {
            "retry_safe_failure"
        } else {
            "outcome_unknown"
        };
        let details = serde_json::json!({"capital_released":false});
        audit::record(
            database,
            &current_trade,
            &broker_name,
            AuditRecord {
                operation_id,
                event_type: "trade.cancel",
                outcome,
                prior_status: current_trade.status,
                resulting_status: current_trade.status,
                details: &details,
            },
        )?;
        return Err(error);
    }

    // 4. Release local capital only after the broker adapter has confirmed cancellation, and
    // persist all accounting changes atomically.
    with_savepoint(database, "persist_broker_cancellation", |database| {
        ensure_snapshot_unchanged(database, &current_trade)?;
        let _ = update_trade_status_and_projection(&current_trade, Status::Canceled, database)?;
        let (tx, account_o, trade_o) =
            commands::transaction::transfer_to_account_from(&current_trade, database)?;
        let details =
            serde_json::json!({"capital_released":true,"transaction_id":tx.id.to_string()});
        audit::record(
            database,
            &current_trade,
            &broker_name,
            AuditRecord {
                operation_id,
                event_type: "trade.cancel",
                outcome: "confirmed",
                prior_status: current_trade.status,
                resulting_status: Status::Canceled,
                details: &details,
            },
        )?;
        Ok((trade_o, account_o, tx))
    })
}

fn pending_order_change(
    database: &mut dyn DatabaseFactory,
    trade: &Trade,
    role: &str,
) -> Result<Option<audit::AuditEvent>, Box<dyn Error>> {
    let logs = database.log_read().read_all_logs_for_trade(trade.id)?;
    let events = audit::ordered_events(&logs)?;
    Ok(events
        .into_iter()
        .rev()
        .find(|event| {
            event.event_type == "order.modify"
                && event
                    .details
                    .get("order_role")
                    .and_then(serde_json::Value::as_str)
                    == Some(role)
        })
        .filter(|event| matches!(event.outcome.as_str(), "attempted" | "outcome_unknown")))
}

// Keep the financial operation and its validation in one reviewable sequence.
#[allow(clippy::too_many_lines)]
pub fn modify_stop(
    trade: &Trade,
    account: &Account,
    new_stop_price: Decimal,
    broker: &mut dyn Broker,
    database: &mut dyn DatabaseFactory,
) -> Result<Trade, Box<dyn std::error::Error>> {
    let (current_trade, account, operation_id, broker_name) = with_savepoint(
        database,
        "stop_intent",
        |database| {
            // 1. Load latest persisted trade snapshot and verify it can be modified.
            let current_trade = latest_trade_snapshot(trade, database)?;
            if pending_order_change(database, &current_trade, "safety_stop")?.is_some() {
                return Err("Previous stop modification is unresolved; sync with the broker before modifying again".into());
            }
            if account.id != current_trade.account_id {
                return Err("Stop modification account mismatch".into());
            }
            crate::validators::trade::can_modify_stop(&current_trade, new_stop_price)?;
            let operation_id = Uuid::new_v4();
            let account = database.account_read().id(current_trade.account_id)?;
            let broker_name = account.broker_kind.as_str().to_string();
            let details = serde_json::json!({"order_role":"safety_stop","old_price":current_trade.safety_stop.unit_price.to_string(),"new_price":new_stop_price.to_string()});
            audit::record(
                database,
                &current_trade,
                &broker_name,
                AuditRecord {
                    operation_id,
                    event_type: "order.modify",
                    outcome: "attempted",
                    prior_status: current_trade.status,
                    resulting_status: current_trade.status,
                    details: &details,
                },
            )?;

            Ok((current_trade, account, operation_id, broker_name))
        },
    )?;
    // 2. Update Trade on the broker
    let new_broker_id = match broker.modify_stop(&current_trade, &account, new_stop_price) {
        Ok(id) => id,
        Err(error) => {
            let outcome = if error
                .downcast_ref::<BrokerError>()
                .is_some_and(BrokerError::is_retry_safe_mutation)
            {
                "retry_safe_failure"
            } else {
                "outcome_unknown"
            };
            let details = serde_json::json!({"order_role":"safety_stop","local_state_changed":false,"new_price":new_stop_price.to_string()});
            audit::record(
                database,
                &current_trade,
                &broker_name,
                AuditRecord {
                    operation_id,
                    event_type: "order.modify",
                    outcome,
                    prior_status: current_trade.status,
                    resulting_status: current_trade.status,
                    details: &details,
                },
            )?;
            return Err(error);
        }
    };

    with_savepoint(database, "stop_acknowledgement", |database| {
        let latest = latest_trade_snapshot(&current_trade, database)?;
        let pending = pending_order_change(database, &latest, "safety_stop")?;
        if pending
            .as_ref()
            .is_none_or(|event| event.operation_id != operation_id.to_string())
        {
            return Ok(latest);
        }
        ensure_snapshot_unchanged(database, &current_trade)?;
        // 3. Modify stop order
        commands::order::modify(
            &current_trade.safety_stop,
            new_stop_price,
            new_broker_id,
            &mut *database.order_write(),
        )?;

        // 4. Refresh Trade
        let trade = database.trade_read().read_trade(current_trade.id)?;
        let details =
            serde_json::json!({"order_role":"safety_stop","new_price":new_stop_price.to_string()});
        audit::record(
            database,
            &trade,
            &broker_name,
            AuditRecord {
                operation_id,
                event_type: "order.modify",
                outcome: "acknowledged",
                prior_status: current_trade.status,
                resulting_status: trade.status,
                details: &details,
            },
        )?;

        Ok(trade)
    })
}

// Keep the broker mutation and its guarded acknowledgement in one sequence.
#[allow(clippy::too_many_lines)]
pub fn modify_target(
    trade: &Trade,
    account: &Account,
    new_price: Decimal,
    broker: &mut dyn Broker,
    database: &mut dyn DatabaseFactory,
) -> Result<Trade, Box<dyn std::error::Error>> {
    let (current_trade, account, operation_id, broker_name) = with_savepoint(
        database,
        "target_intent",
        |database| {
            // 1. Load latest persisted trade snapshot and verify it can be modified.
            let current_trade = latest_trade_snapshot(trade, database)?;
            crate::validators::trade::can_modify_target(&current_trade)?;
            if pending_order_change(database, &current_trade, "target")?.is_some() {
                return Err("Previous target modification is unresolved; sync with the broker before modifying again".into());
            }
            let operation_id = Uuid::new_v4();
            if account.id != current_trade.account_id {
                return Err("Target modification account mismatch".into());
            }
            if new_price <= Decimal::ZERO {
                return Err("Target price must be positive".into());
            }
            let account = database.account_read().id(current_trade.account_id)?;
            let broker_name = account.broker_kind.as_str().to_string();
            let details = serde_json::json!({"order_role":"target","old_price":current_trade.target.unit_price.to_string(),"new_price":new_price.to_string()});
            audit::record(
                database,
                &current_trade,
                &broker_name,
                AuditRecord {
                    operation_id,
                    event_type: "order.modify",
                    outcome: "attempted",
                    prior_status: current_trade.status,
                    resulting_status: current_trade.status,
                    details: &details,
                },
            )?;

            Ok((current_trade, account, operation_id, broker_name))
        },
    )?;

    // 2. Update Trade on the broker
    let new_broker_id = match broker.modify_target(&current_trade, &account, new_price) {
        Ok(id) => id,
        Err(error) => {
            let outcome = if error
                .downcast_ref::<BrokerError>()
                .is_some_and(BrokerError::is_retry_safe_mutation)
            {
                "retry_safe_failure"
            } else {
                "outcome_unknown"
            };
            let details = serde_json::json!({"order_role":"target","local_state_changed":false,"new_price":new_price.to_string()});
            audit::record(
                database,
                &current_trade,
                &broker_name,
                AuditRecord {
                    operation_id,
                    event_type: "order.modify",
                    outcome,
                    prior_status: current_trade.status,
                    resulting_status: current_trade.status,
                    details: &details,
                },
            )?;
            return Err(error);
        }
    };

    with_savepoint(database, "target_acknowledgement", |database| {
        let latest = latest_trade_snapshot(&current_trade, database)?;
        if pending_order_change(database, &latest, "target")?
            .as_ref()
            .is_none_or(|event| event.operation_id != operation_id.to_string())
        {
            return Ok(latest);
        }
        ensure_snapshot_unchanged(database, &current_trade)?;
        // 3. Modify target order
        commands::order::modify(
            &current_trade.target,
            new_price,
            new_broker_id,
            &mut *database.order_write(),
        )?;

        // 4. Refresh Trade
        let trade = database.trade_read().read_trade(current_trade.id)?;
        let details = serde_json::json!({"order_role":"target","new_price":new_price.to_string()});
        audit::record(
            database,
            &trade,
            &broker_name,
            AuditRecord {
                operation_id,
                event_type: "order.modify",
                outcome: "acknowledged",
                prior_status: current_trade.status,
                resulting_status: trade.status,
                details: &details,
            },
        )?;

        Ok(trade)
    })
}

pub fn fund(
    trade: &Trade,
    database: &mut dyn DatabaseFactory,
) -> Result<(Trade, Transaction, AccountBalance, TradeBalance), Box<dyn std::error::Error>> {
    with_savepoint(database, "fund", |database| {
        // 1. Load latest persisted trade snapshot to prevent stale-state funding.
        let current_trade = latest_trade_snapshot(trade, database)?;

        // 2. Validate status and funding constraints against the persisted trade.
        crate::validators::trade::can_fund(&current_trade)?;
        crate::validators::funding::can_fund(&current_trade, database)?;

        // 3. Update trade status to funded
        let mut funded_trade =
            update_trade_status_and_projection(&current_trade, Status::Funded, database)?;

        // 4. Create transaction to fund the trade
        let (transaction, account_balance, trade_balance) =
            commands::transaction::transfer_to_fund_trade(&current_trade, database)?;
        // Keep the returned trade snapshot consistent with persisted projections.
        funded_trade.balance = trade_balance.clone();

        // 5. Return data objects
        Ok((funded_trade, transaction, account_balance, trade_balance))
    })
}

// Keep the external mutation and its durable intent/outcome records in one linear review surface.
#[allow(clippy::too_many_lines)]
pub fn submit(
    trade: &Trade,
    database: &mut dyn DatabaseFactory,
    broker: &mut dyn Broker,
) -> Result<(Trade, BrokerLog), Box<dyn std::error::Error>> {
    let (current_trade, account, mut submitted, operation_id, broker_name) =
        with_savepoint(database, "submit_intent", |database| {
            // 1. Load latest persisted trade snapshot to prevent stale-state submission.
            let current_trade = latest_trade_snapshot(trade, database)?;

            // 2. Validate that Trade can be submitted
            crate::validators::trade::can_submit(&current_trade)?;
            let operation_id = Uuid::new_v4();
            let broker_name = database
                .account_read()
                .id(current_trade.account_id)?
                .broker_kind
                .as_str()
                .to_string();
            let attempt_details = serde_json::json!({
                "entry_order_ref": current_trade.entry.id.to_string(),
                "target_order_ref": current_trade.target.id.to_string(),
                "stop_order_ref": current_trade.safety_stop.id.to_string(),
                "quantity": current_trade.entry.quantity.to_string()
            });
            audit::record(
                database,
                &current_trade,
                &broker_name,
                AuditRecord {
                    operation_id,
                    event_type: "trade.submit",
                    outcome: "attempted",
                    prior_status: current_trade.status,
                    resulting_status: Status::Submitted,
                    details: &attempt_details,
                },
            )?;

            // 3. Persist a durable submission intent before crossing the external boundary. A crash or
            // local acknowledgement failure must leave the trade non-resubmittable and reconcilable.
            let account = database.account_read().id(current_trade.account_id)?;
            let mut submitted = database
                .trade_write()
                .update_trade_status(Status::Submitted, &current_trade)?;
            submitted.balance = database
                .trade_read()
                .read_trade_balance(current_trade.balance.id)?;
            Ok((current_trade, account, submitted, operation_id, broker_name))
        })?;
    let (log, order_id) = match broker.submit_trade(&current_trade, &account) {
        Ok(acknowledgement) => acknowledgement,
        Err(error)
            if error
                .downcast_ref::<BrokerError>()
                .is_some_and(BrokerError::is_retry_safe_mutation) =>
        {
            with_savepoint(database, "submit_retry_safe", |database| {
                ensure_snapshot_unchanged(database, &submitted)?;
                database
                    .trade_write()
                    .update_trade_status(Status::Funded, &submitted)?;
                Ok(())
            })?;
            let details = serde_json::json!({"intent_retained":false,"safe_to_retry":true});
            audit::record(
                database,
                &current_trade,
                &broker_name,
                AuditRecord {
                    operation_id,
                    event_type: "trade.submit",
                    outcome: "retry_safe_failure",
                    prior_status: Status::Submitted,
                    resulting_status: Status::Funded,
                    details: &details,
                },
            )?;
            return Err(error);
        }
        Err(error) => {
            let details = serde_json::json!({"intent_retained":true,"safe_to_retry":false});
            audit::record(
                database,
                &submitted,
                &broker_name,
                AuditRecord {
                    operation_id,
                    event_type: "trade.submit",
                    outcome: "outcome_unknown",
                    prior_status: Status::Funded,
                    resulting_status: Status::Submitted,
                    details: &details,
                },
            )?;
            return Err(format!(
                "trade {} submission outcome is unknown; durable Submitted intent retained; do not resubmit; reconcile with the broker: {error}",
                current_trade.id
            )
            .into());
        }
    };

    // 4. Persist acknowledgement details atomically. The durable Submitted state remains in place
    // if this fails, blocking duplicate submission while reconciliation repairs missing IDs/logs.
    let persisted = with_savepoint(database, "persist_broker_submission", |database| {
        ensure_snapshot_unchanged(database, &submitted)?;
        database
            .log_write()
            .create_log(log.log.as_str(), &current_trade)?;
        let _ = commands::balance::apply_account_projection_for_trade_status_transition(
            database,
            &current_trade,
            Status::Submitted,
        )?;

        let mut order_write = database.order_write();
        submitted.safety_stop = order_write.submit_of(&submitted.safety_stop, order_id.stop)?;
        submitted.entry = order_write.submit_of(&submitted.entry, order_id.entry)?;
        submitted.target = order_write.submit_of(&submitted.target, order_id.target)?;
        let details = serde_json::json!({"broker_order_refs_persisted":true});
        audit::record(database, &submitted, &broker_name, AuditRecord {
            operation_id,
            event_type: "trade.submit",
            outcome: "acknowledged",
            prior_status: Status::Funded,
            resulting_status: Status::Submitted,
            details: &details,
        })?;
        Ok(submitted)
    })
    .map_err(|error| {
        format!(
            "broker accepted trade {} but local acknowledgement persistence failed; do not resubmit; reconcile with the broker: {error}",
            current_trade.id
        )
    })?;

    Ok((persisted, log))
}

// Reconciliation is deliberately linear so audit and savepoint ordering remain reviewable.
#[allow(clippy::too_many_lines)]
pub fn sync_with_broker(
    trade: &Trade,
    account: &Account,
    database: &mut dyn DatabaseFactory,
    broker: &mut dyn Broker,
) -> SyncWithBrokerResult {
    let polled_trade = latest_trade_snapshot(trade, database)?;
    if account.id != polled_trade.account_id {
        return Err("Sync account identity mismatch".into());
    }
    let persisted_account = database.account_read().id(polled_trade.account_id)?;
    let account = &persisted_account;
    let trade = &polled_trade;
    let broker_name = database
        .account_read()
        .id(trade.account_id)?
        .broker_kind
        .as_str()
        .to_string();
    // Best-effort execution reconciliation window.
    // This is intentionally outside the DB savepoint because it may involve broker I/O.
    let after = database
        .execution_read()
        .latest_trade_execution_at(trade.id)?;
    let after =
        after.map(|t| chrono::DateTime::<chrono::Utc>::from_naive_utc_and_offset(t, chrono::Utc));
    // 1. Sync Trade with Broker
    let (status, orders, log) = broker.sync_trade(trade, account)?;
    let operation_id = Uuid::new_v4();

    let reconciled_executions = match broker.fetch_executions(trade, account, after) {
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
    let fees = match broker.fetch_fee_activities(trade, account, None) {
        Ok(fees) => fees,
        Err(e) => {
            eprintln!("Fee reconciliation failed for trade {}: {e}", trade.id);
            vec![]
        }
    };
    // 2. Persist the whole sync lifecycle atomically.
    let (transitioned_to_closed, persisted_trade) = with_savepoint(
        database,
        "sync_trade_lifecycle",
        |database| {
            // 2.1 Resolve broker updates against the latest persisted trade snapshot.
            let current_trade = database.trade_read().read_trade(trade.id)?;
            if current_trade.updated_at != polled_trade.updated_at
                || [
                    (&current_trade.entry, &polled_trade.entry),
                    (&current_trade.target, &polled_trade.target),
                    (&current_trade.safety_stop, &polled_trade.safety_stop),
                ]
                .iter()
                .any(|(current, polled)| current.updated_at != polled.updated_at)
            {
                return Err(
                    "Trade changed during broker reconciliation; sync again using a fresh snapshot"
                        .into(),
                );
            }
            let persisted_status = current_trade.status;
            let was_closed = persisted_status == Status::ClosedTarget
                || persisted_status == Status::ClosedStopLoss;
            let resolved = resolve_orders_for_sync(&current_trade, &orders)?;

            // 2.3 Validate payload before mutating DB state.
            validate_sync_payload(&current_trade, account, status, &resolved)?;

            for (role, resolved_order) in [
                ("safety_stop", &resolved.stop),
                ("target", &resolved.target),
            ] {
                if orders.iter().any(|order| order.id == resolved_order.id) {
                    if let Some(pending) = pending_order_change(database, &current_trade, role)? {
                        if pending
                            .details
                            .get("new_price")
                            .and_then(serde_json::Value::as_str)
                            .and_then(|price| price.parse::<Decimal>().ok())
                            == Some(resolved_order.unit_price)
                        {
                            audit::record(
                                database,
                                &current_trade,
                                &broker_name,
                                AuditRecord {
                                    operation_id: Uuid::parse_str(&pending.operation_id)?,
                                    event_type: "order.modify",
                                    outcome: "reconciled",
                                    prior_status: current_trade.status,
                                    resulting_status: status,
                                    details: &serde_json::json!({"order_role":role,"new_price":resolved_order.unit_price.to_string()}),
                                },
                            )?;
                        }
                    }
                }
            }
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
            let mut normalized_executions = derive_trade_update_executions(
                &current_trade,
                &resolved,
                account.id,
                &trade.trading_vehicle.symbol,
                account.broker_kind,
            );
            let mut trade_with_synced_orders = current_trade;
            trade_with_synced_orders.entry = resolved.entry;
            trade_with_synced_orders.target = resolved.target;
            trade_with_synced_orders.safety_stop = resolved.stop;
            normalized_executions.extend(reconciled_executions.clone());
            let trade =
                super::reconciliation::reconcile(database, &trade_with_synced_orders, status)?;
            // 2.6 Persist executions (fills) for execution-level accounting, idempotently.
            //
            // Important: we only attribute executions to this trade when the broker order ID matches
            // one of the trade orders. This prevents cross-trade contamination on the same symbol.
            persist_executions_for_trade(database, &trade, &normalized_executions)?;
            reconcile_fee_activities_for_trade(database, &trade, &fees)?;

            // 2.6 Ensure sibling exit orders are consistently closed in terminal states.
            reconcile_sibling_exit_orders(&trade, trade.status, database)?;
            let transitioned_to_closed =
                matches!(trade.status, Status::ClosedTarget | Status::ClosedStopLoss)
                    && !was_closed;
            let details = serde_json::json!({
                "orders_received": orders.len(),
                "executions_reconciled": reconciled_executions.len(),
                "fees_reconciled": fees.len(),
                "broker_response_sha256": sha2::Sha256::digest(log.log.as_bytes()).iter().map(|byte| format!("{byte:02x}")).collect::<String>()
            });
            audit::record(
                database,
                &trade,
                &broker_name,
                AuditRecord {
                    operation_id,
                    event_type: "trade.sync",
                    outcome: "reconciled",
                    prior_status: trade_with_synced_orders.status,
                    resulting_status: trade.status,
                    details: &details,
                },
            )?;
            Ok((transitioned_to_closed, trade))
        },
    )?;

    Ok((
        persisted_trade.status,
        orders,
        log,
        transitioned_to_closed,
        persisted_trade,
    ))
}

fn persist_executions_for_trade(
    database: &mut dyn DatabaseFactory,
    trade: &Trade,
    executions: &[model::Execution],
) -> Result<(), Box<dyn Error>> {
    let trade_symbol = trade.trading_vehicle.symbol.as_str();
    let entry_broker_id = trade.entry.broker_order_id.as_deref();
    let target_broker_id = trade.target.broker_order_id.as_deref();
    let stop_broker_id = trade.safety_stop.broker_order_id.as_deref();

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
        if must_reject_fractional_qty(trade) && !is_integer_decimal(exec.qty) {
            return Err(format!(
                "fractional execution qty is not allowed for non-fractionable security {}: {}",
                trade.trading_vehicle.symbol, exec.qty
            )
            .into());
        }
        if exec.symbol != trade_symbol {
            // Broker impl should have filtered, but keep this as a hard safety check.
            continue;
        }

        let Some(broker_order_id) = exec.broker_order_id.as_deref() else {
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

fn ensure_snapshot_unchanged(
    database: &mut dyn DatabaseFactory,
    expected: &Trade,
) -> Result<(), Box<dyn Error>> {
    let current = database.trade_read().read_trade(expected.id)?;
    if current.updated_at != expected.updated_at
        || current.status != expected.status
        || [
            (&current.entry, &expected.entry),
            (&current.target, &expected.target),
            (&current.safety_stop, &expected.safety_stop),
        ]
        .iter()
        .any(|(current, expected)| current.updated_at != expected.updated_at)
    {
        return Err(
            "Trade changed while awaiting broker response; preserve newer state and sync again"
                .into(),
        );
    }
    Ok(())
}

fn derive_trade_update_executions(
    previous_trade: &Trade,
    resolved: &ResolvedSyncOrders,
    account_id: uuid::Uuid,
    symbol: &str,
    broker_kind: BrokerKind,
) -> Vec<Execution> {
    let mut out = Vec::new();
    for (previous_order, updated_order, side) in [
        (
            &previous_trade.entry,
            &resolved.entry,
            side_for_entry(previous_trade.category),
        ),
        (
            &previous_trade.target,
            &resolved.target,
            side_for_exit(previous_trade.category),
        ),
        (
            &previous_trade.safety_stop,
            &resolved.stop,
            side_for_exit(previous_trade.category),
        ),
    ] {
        if updated_order.filled_quantity <= Decimal::ZERO
            || updated_order.filled_quantity <= previous_order.filled_quantity
        {
            continue;
        }
        let Some(broker_order_id) = updated_order.broker_order_id.clone() else {
            continue;
        };
        let Some(average_price) = updated_order.average_filled_price else {
            continue;
        };
        let executed_at = updated_order
            .filled_at
            .unwrap_or_else(|| chrono::Utc::now().naive_utc());
        let Some(delta_qty) = updated_order
            .filled_quantity
            .checked_sub(previous_order.filled_quantity)
        else {
            continue;
        };
        // A cumulative average is not the price of the newest fill. Recover
        // that fill's cash delta before deriving its unit price.
        let previous_cash = if previous_order.filled_quantity.is_zero() {
            Some(Decimal::ZERO)
        } else {
            previous_order
                .average_filled_price
                .and_then(|price| price.checked_mul(previous_order.filled_quantity))
        };
        let price = average_price
            .checked_mul(updated_order.filled_quantity)
            .and_then(|cash| previous_cash.and_then(|prior| cash.checked_sub(prior)))
            .and_then(|cash| cash.checked_div(delta_qty));
        let Some(price) = price.filter(|value| *value > Decimal::ZERO) else {
            continue;
        };
        let qty = delta_qty;
        let broker_execution_id = format!(
            "trade_updates:{}:{}:{}",
            broker_order_id,
            updated_order.filled_quantity,
            executed_at.and_utc().timestamp_millis()
        );
        let mut exec = Execution::new(
            broker_kind.as_str().to_string(),
            ExecutionSource::TradeUpdates,
            account_id,
            broker_execution_id,
            Some(broker_order_id),
            symbol.to_string(),
            side,
            qty,
            price,
            executed_at,
        );
        exec.raw_json = None;
        out.push(exec);
    }
    out
}

fn side_for_entry(category: TradeCategory) -> ExecutionSide {
    match category {
        TradeCategory::Long => ExecutionSide::Buy,
        TradeCategory::Short => ExecutionSide::SellShort,
    }
}

fn side_for_exit(category: TradeCategory) -> ExecutionSide {
    match category {
        TradeCategory::Long => ExecutionSide::Sell,
        TradeCategory::Short => ExecutionSide::Buy,
    }
}

fn must_reject_fractional_qty(trade: &Trade) -> bool {
    matches!(
        trade.trading_vehicle.category,
        TradingVehicleCategory::Stock | TradingVehicleCategory::Etf | TradingVehicleCategory::Bond
    ) && trade.trading_vehicle.fractionable != Some(true)
}

fn is_integer_decimal(value: Decimal) -> bool {
    value.fract().is_zero()
}

fn fee_role(trade: &Trade, fee: &FeeActivity) -> Option<bool> {
    fee.broker_order_id.as_deref().and_then(|id| {
        if trade.entry.broker_order_id.as_deref() == Some(id) {
            Some(true)
        } else if trade.target.broker_order_id.as_deref() == Some(id)
            || trade.safety_stop.broker_order_id.as_deref() == Some(id)
        {
            Some(false)
        } else {
            None
        }
    })
}

fn reconcile_fee_activities_for_trade(
    database: &mut dyn DatabaseFactory,
    trade: &Trade,
    fees: &[FeeActivity],
) -> Result<(), Box<dyn Error>> {
    let broker = database.account_read().id(trade.account_id)?.broker_kind;
    for fee in fees {
        if fee.account_id != trade.account_id || fee.broker != broker.as_str() {
            return Err("Fee broker/account identity mismatch".into());
        }
        let role = fee_role(trade, fee);
        // Account-level or ambiguous activities are retained without charging any trade.
        let first_allocation = database
            .execution_write()
            .record_fee_activity(fee, role.map(|_| trade.id))?;
        if first_allocation {
            match role {
                Some(true) => {
                    commands::transaction::transfer_opening_fee(fee.amount, trade, database)?;
                }
                Some(false) => {
                    commands::transaction::transfer_closing_fee(fee.amount, trade, database)?;
                }
                None => {}
            }
        }
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
    for (old, new) in [
        (&trade.entry, &resolved.entry),
        (&trade.target, &resolved.target),
        (&trade.safety_stop, &resolved.stop),
    ] {
        if new.quantity <= Decimal::ZERO
            || new.unit_price <= Decimal::ZERO
            || new.filled_quantity < old.filled_quantity
        {
            return Err("Invalid or regressing broker order snapshot".into());
        }
    }

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
            ensure_order_filled(&resolved.target, "target")?;
        }
        Status::ClosedStopLoss => {
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
        || current.unit_price != resolved.unit_price
        || current.quantity != resolved.quantity
}

fn validate_sync_transition(trade: &Trade, status: Status) -> Result<(), Box<dyn Error>> {
    if matches!(
        trade.status,
        Status::Submitted | Status::PartiallyFilled | Status::Filled
    ) && matches!(
        status,
        Status::PartiallyFilled
            | Status::Filled
            | Status::Canceled
            | Status::Expired
            | Status::Rejected
            | Status::ClosedTarget
            | Status::ClosedStopLoss
    ) {
        return Ok(());
    }
    if trade.status == status
        && matches!(
            status,
            Status::Canceled | Status::Expired | Status::Rejected
        )
    {
        return Ok(());
    }
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
    merged.broker_order_id = update.broker_order_id.clone();
    merged.status = update.status;
    merged.unit_price = update.unit_price;
    merged.quantity = update.quantity;
    merged.category = update.category;
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
        Status::ClosedTarget if !is_terminal_order_status(trade.safety_stop.status) => {
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
        Status::ClosedStopLoss if !is_terminal_order_status(trade.target.status) => {
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

#[cfg(test)]
mod tests {
    use super::{
        derive_trade_update_executions, ensure_order_filled, fee_role, is_integer_decimal,
        is_terminal_order_status, must_reject_fractional_qty, resolve_orders_for_sync,
        should_persist_order_update, validate_broker_asset_support, validate_sync_transition,
        ResolvedSyncOrders,
    };
    use model::{
        BrokerError, BrokerKind, Currency, DraftTrade, ExecutionSource, FeeActivity, OrderCategory,
        OrderStatus, Status, Trade, TradeCategory, TradingVehicleCategory,
    };
    use rust_decimal_macros::dec;
    use uuid::Uuid;

    fn sample_fee(
        amount: rust_decimal::Decimal,
        symbol: Option<&str>,
        day: chrono::NaiveDate,
    ) -> FeeActivity {
        FeeActivity {
            broker: "alpaca".to_string(),
            broker_activity_id: Uuid::new_v4().to_string(),
            account_id: Uuid::new_v4(),
            broker_order_id: None,
            symbol: symbol.map(str::to_string),
            activity_type: "FEE".to_string(),
            amount,
            occurred_at: day.and_hms_opt(12, 0, 0).unwrap(),
            raw_json: None,
        }
    }

    #[test]
    fn test_integer_decimal_detection() {
        assert!(is_integer_decimal(dec!(10)));
        assert!(is_integer_decimal(dec!(10.000)));
        assert!(!is_integer_decimal(dec!(10.25)));
    }

    #[test]
    fn test_fractional_qty_policy_for_security_categories() {
        let mut trade = Trade::default();
        trade.trading_vehicle.category = TradingVehicleCategory::Stock;
        trade.trading_vehicle.fractionable = Some(false);
        assert!(must_reject_fractional_qty(&trade));

        trade.trading_vehicle.fractionable = Some(true);
        assert!(!must_reject_fractional_qty(&trade));

        trade.trading_vehicle.category = TradingVehicleCategory::Etf;
        trade.trading_vehicle.fractionable = Some(false);
        assert!(must_reject_fractional_qty(&trade));

        trade.trading_vehicle.category = TradingVehicleCategory::Bond;
        trade.trading_vehicle.fractionable = None;
        assert!(must_reject_fractional_qty(&trade));

        trade.trading_vehicle.fractionable = Some(true);
        assert!(!must_reject_fractional_qty(&trade));

        trade.trading_vehicle.category = TradingVehicleCategory::Crypto;
        trade.trading_vehicle.fractionable = Some(false);
        assert!(!must_reject_fractional_qty(&trade));
    }

    #[test]
    fn validate_broker_asset_support_rejects_alpaca_bonds() {
        let mut draft = DraftTrade {
            account: model::Account::default(),
            trading_vehicle: model::TradingVehicle::default(),
            quantity: dec!(1),
            currency: Currency::USD,
            category: TradeCategory::Long,
            thesis: None,
            sector: None,
            asset_class: None,
            context: None,
        };
        draft.trading_vehicle.category = TradingVehicleCategory::Bond;
        draft.trading_vehicle.broker = "alpaca".to_string();

        let error =
            validate_broker_asset_support(&draft).expect_err("alpaca bond should be rejected");

        assert!(error
            .to_string()
            .contains("Bonds are not supported by Alpaca"));
        assert!(matches!(
            error.downcast_ref::<BrokerError>(),
            Some(BrokerError::UnsupportedAssetClass {
                broker: BrokerKind::Alpaca,
                category: TradingVehicleCategory::Bond,
                ..
            })
        ));
    }

    #[test]
    fn fees_without_order_identity_are_not_allocated() {
        let mut trade = Trade::default();
        trade.trading_vehicle.symbol = "AAPL".to_string();
        trade.entry.filled_at = Some(
            chrono::NaiveDate::from_ymd_opt(2026, 2, 10)
                .unwrap()
                .and_hms_opt(12, 0, 0)
                .unwrap(),
        );
        trade.target.filled_at = Some(
            chrono::NaiveDate::from_ymd_opt(2026, 2, 11)
                .unwrap()
                .and_hms_opt(12, 0, 0)
                .unwrap(),
        );
        trade.status = model::Status::ClosedTarget;

        let entry_day = trade.entry.filled_at.unwrap().date();
        let exit_day = trade.target.filled_at.unwrap().date();
        let fees = [
            sample_fee(dec!(1.25), Some("AAPL"), entry_day),
            sample_fee(dec!(0.75), Some("AAPL"), exit_day),
            sample_fee(dec!(9.99), Some("MSFT"), exit_day),
        ];

        assert!(fees.iter().all(|fee| fee_role(&trade, fee).is_none()));
    }

    #[test]
    fn test_should_persist_order_update_detects_material_fields() {
        let current = Trade::default().entry;
        assert!(!should_persist_order_update(&current, &current));

        let mut changed = current.clone();
        changed.broker_order_id = Some("broker-entry".to_string());
        assert!(should_persist_order_update(&current, &changed));

        let mut changed = current.clone();
        changed.status = OrderStatus::Filled;
        assert!(should_persist_order_update(&current, &changed));

        let mut changed = current.clone();
        changed.filled_quantity = 1.into();
        assert!(should_persist_order_update(&current, &changed));

        let mut changed = current.clone();
        changed.average_filled_price = Some(dec!(100));
        assert!(should_persist_order_update(&current, &changed));

        let now = chrono::Utc::now().naive_utc();
        let mut changed = current.clone();
        changed.submitted_at = Some(now);
        assert!(should_persist_order_update(&current, &changed));

        let mut changed = current.clone();
        changed.filled_at = Some(now);
        assert!(should_persist_order_update(&current, &changed));

        let mut changed = current.clone();
        changed.expired_at = Some(now);
        assert!(should_persist_order_update(&current, &changed));

        let mut changed = current.clone();
        changed.cancelled_at = Some(now);
        assert!(should_persist_order_update(&current, &changed));

        let mut changed = current.clone();
        changed.closed_at = Some(now);
        assert!(should_persist_order_update(&current, &changed));

        let mut changed = current.clone();
        changed.category = OrderCategory::Stop;
        assert!(should_persist_order_update(&current, &changed));
    }

    #[test]
    fn test_terminal_order_statuses_are_classified() {
        assert!([
            OrderStatus::Filled,
            OrderStatus::Canceled,
            OrderStatus::Expired,
            OrderStatus::Rejected,
        ]
        .into_iter()
        .all(is_terminal_order_status));
        assert!([OrderStatus::New, OrderStatus::PartiallyFilled]
            .into_iter()
            .all(|status| !is_terminal_order_status(status)));
    }

    #[test]
    fn test_validate_sync_transition_allows_only_safe_state_progression() {
        let mut trade = Trade {
            status: Status::Submitted,
            ..Trade::default()
        };
        assert!(validate_sync_transition(&trade, Status::Submitted).is_ok());
        assert!(validate_sync_transition(&trade, Status::Filled).is_ok());
        assert!(validate_sync_transition(&trade, Status::ClosedTarget).is_ok());
        assert!(validate_sync_transition(&trade, Status::ClosedStopLoss).is_ok());

        trade.status = Status::Filled;
        assert!(validate_sync_transition(&trade, Status::Filled).is_ok());
        assert!(validate_sync_transition(&trade, Status::ClosedTarget).is_ok());
        assert!(validate_sync_transition(&trade, Status::ClosedStopLoss).is_ok());

        trade.status = Status::Canceled;
        assert!(validate_sync_transition(&trade, Status::ClosedTarget).is_ok());

        trade.status = Status::New;
        let error = validate_sync_transition(&trade, Status::Filled)
            .expect_err("new trade must not sync directly to filled");
        assert!(error.to_string().contains("invalid sync transition"));
    }

    #[test]
    fn test_ensure_order_filled_requires_filled_status_and_average_price() {
        let mut order = Trade::default().entry;

        let error =
            ensure_order_filled(&order, "entry").expect_err("non-filled order should be rejected");
        assert!(error.to_string().contains("expected entry order"));

        order.status = OrderStatus::Filled;
        let error = ensure_order_filled(&order, "entry")
            .expect_err("filled order without price should be rejected");
        assert!(error.to_string().contains("has no average filled price"));

        order.average_filled_price = Some(dec!(100));
        assert!(ensure_order_filled(&order, "entry").is_ok());
    }

    #[test]
    fn test_resolve_orders_for_sync_merges_known_orders_and_rejects_invalid_payloads() {
        let trade = Trade::default();
        let now = chrono::Utc::now().naive_utc();

        let mut entry_update = trade.entry.clone();
        entry_update.broker_order_id = Some("entry-broker".to_string());
        entry_update.status = OrderStatus::Filled;
        entry_update.filled_quantity = 10.into();
        entry_update.average_filled_price = Some(dec!(100.25));
        entry_update.submitted_at = Some(now);
        entry_update.filled_at = Some(now);

        let mut target_update = trade.target.clone();
        target_update.broker_order_id = Some("target-broker".to_string());
        target_update.status = OrderStatus::New;

        let mut stop_update = trade.safety_stop.clone();
        stop_update.broker_order_id = Some("stop-broker".to_string());
        stop_update.status = OrderStatus::New;

        let resolved = resolve_orders_for_sync(
            &trade,
            &[
                entry_update.clone(),
                target_update.clone(),
                stop_update.clone(),
            ],
        )
        .expect("known order updates should resolve");
        assert_eq!(
            resolved.entry.broker_order_id.as_deref(),
            Some("entry-broker")
        );
        assert_eq!(resolved.entry.status, OrderStatus::Filled);
        assert_eq!(resolved.entry.filled_quantity, dec!(10));
        assert_eq!(resolved.entry.average_filled_price, Some(dec!(100.25)));
        assert_eq!(
            resolved.target.broker_order_id.as_deref(),
            Some("target-broker")
        );
        assert_eq!(
            resolved.stop.broker_order_id.as_deref(),
            Some("stop-broker")
        );

        let error = resolve_orders_for_sync(&trade, &[entry_update.clone(), entry_update])
            .err()
            .expect("duplicate order update should be rejected");
        assert!(error.to_string().contains("duplicate order update"));

        let mut unknown_update = trade.entry.clone();
        unknown_update.id = Uuid::new_v4();
        let error = resolve_orders_for_sync(&trade, &[unknown_update])
            .err()
            .expect("unknown order update should be rejected");
        assert!(error.to_string().contains("not found in trade"));
    }

    #[test]
    #[allow(clippy::field_reassign_with_default)]
    fn test_derive_trade_update_executions_normalizes_entry_and_exit() {
        let mut previous = Trade {
            category: TradeCategory::Long,
            ..Trade::default()
        };
        previous.trading_vehicle.symbol = "AAPL".to_string();
        previous.entry.broker_order_id = Some(Uuid::new_v4().to_string());
        previous.entry.filled_quantity = 5.into();
        previous.entry.average_filled_price = Some(dec!(100.50));

        let mut resolved = ResolvedSyncOrders {
            entry: previous.entry.clone(),
            target: previous.target.clone(),
            stop: previous.safety_stop.clone(),
        };
        resolved.entry.filled_quantity = 10.into();
        resolved.entry.average_filled_price = Some(dec!(101));
        resolved.entry.filled_at = Some(
            chrono::NaiveDate::from_ymd_opt(2026, 2, 18)
                .unwrap()
                .and_hms_opt(10, 0, 0)
                .unwrap(),
        );
        resolved.target.broker_order_id = Some(Uuid::new_v4().to_string());
        resolved.target.filled_quantity = 10.into();
        resolved.target.average_filled_price = Some(dec!(102.75));
        resolved.target.filled_at = Some(
            chrono::NaiveDate::from_ymd_opt(2026, 2, 18)
                .unwrap()
                .and_hms_opt(11, 0, 0)
                .unwrap(),
        );

        let rows = derive_trade_update_executions(
            &previous,
            &resolved,
            Uuid::new_v4(),
            "AAPL",
            BrokerKind::Ibkr,
        );
        assert_eq!(rows.len(), 2);
        assert!(rows
            .iter()
            .all(|row| row.source == ExecutionSource::TradeUpdates));
        assert!(rows.iter().all(|row| row.broker == "ibkr"));
        assert!(rows.iter().all(|row| row.symbol == "AAPL"));
        assert!(rows
            .iter()
            .any(|row| row.qty == dec!(5) && row.price == dec!(101.5)));
        assert!(rows.iter().any(|row| row.qty == dec!(10)));
    }

    #[test]
    #[allow(clippy::field_reassign_with_default)]
    fn test_derive_trade_update_executions_skips_unfilled_orders() {
        let trade = Trade::default();
        let resolved = ResolvedSyncOrders {
            entry: trade.entry.clone(),
            target: trade.target.clone(),
            stop: trade.safety_stop.clone(),
        };
        let rows = derive_trade_update_executions(
            &trade,
            &resolved,
            Uuid::new_v4(),
            "AAPL",
            BrokerKind::Alpaca,
        );
        assert!(rows.is_empty());
    }
}

// Keep the external mutation and its durable intent/outcome records in one linear review surface.
#[allow(clippy::too_many_lines)]
pub fn close(
    trade: &Trade,
    database: &mut dyn DatabaseFactory,
    broker: &mut dyn Broker,
) -> Result<(TradeBalance, BrokerLog), Box<dyn std::error::Error>> {
    let (current_trade, pending_trade, account, operation_id, broker_name) =
        with_savepoint(database, "close_intent", |database| {
            // 1. Load latest persisted trade snapshot to prevent stale-state replay.
            let current_trade = latest_trade_snapshot(trade, database)?;

            // 2. Verify trade can be closed
            crate::validators::trade::can_close(&current_trade)?;
            if pending_order_change(database, &current_trade, "target")?.is_some() {
                return Err(
                    "Previous target modification is unresolved; sync before requesting a close"
                        .into(),
                );
            }
            let operation_id = Uuid::new_v4();
            let broker_name = database
                .account_read()
                .id(current_trade.account_id)?
                .broker_kind
                .as_str()
                .to_string();
            let attempt_details = serde_json::json!({
                "target_order_ref": current_trade.target.id.to_string(),
                "protective_stop_retention": "adapter_must_preserve",
                "quantity": current_trade.entry.quantity.to_string()
            });
            audit::record(
                database,
                &current_trade,
                &broker_name,
                AuditRecord {
                    operation_id,
                    event_type: "trade.close",
                    outcome: "attempted",
                    prior_status: current_trade.status,
                    resulting_status: current_trade.status,
                    details: &attempt_details,
                },
            )?;

            // 3. Persist the market-exit intent before crossing the external boundary. Even when the
            // acknowledgement is lost, stale callers must be unable to issue another close.
            let mut pending_target = current_trade.target.clone();
            pending_target.category = OrderCategory::Market;
            database.order_write().update(&pending_target)?;

            // 4. Submit the market-exit request. Errors are outcome-ambiguous, so retain the durable intent.
            let account = database.account_read().id(current_trade.account_id)?;
            let pending_trade = database.trade_read().read_trade(current_trade.id)?;
            Ok((
                current_trade,
                pending_trade,
                account,
                operation_id,
                broker_name,
            ))
        })?;
    let (mut target_order, log) = match broker.close_trade(&current_trade, &account) {
        Ok(acknowledgement) => acknowledgement,
        Err(error)
            if error
                .downcast_ref::<BrokerError>()
                .is_some_and(BrokerError::is_retry_safe_mutation) =>
        {
            with_savepoint(database, "close_retry_safe", |database| {
                ensure_snapshot_unchanged(database, &pending_trade)?;
                database.order_write().update(&current_trade.target)?;
                Ok(())
            })?;
            let details = serde_json::json!({"intent_retained":false,"safe_to_retry":true});
            audit::record(
                database,
                &current_trade,
                &broker_name,
                AuditRecord {
                    operation_id,
                    event_type: "trade.close",
                    outcome: "retry_safe_failure",
                    prior_status: current_trade.status,
                    resulting_status: current_trade.status,
                    details: &details,
                },
            )?;
            return Err(error);
        }
        Err(error) => {
            let details = serde_json::json!({"intent_retained":true,"safe_to_retry":false});
            audit::record(
                database,
                &current_trade,
                &broker_name,
                AuditRecord {
                    operation_id,
                    event_type: "trade.close",
                    outcome: "outcome_unknown",
                    prior_status: current_trade.status,
                    resulting_status: current_trade.status,
                    details: &details,
                },
            )?;
            return Err(format!(
                "trade {} close outcome is unknown; durable market-exit intent retained; do not retry; reconcile with the broker: {error}",
                current_trade.id
            )
            .into());
        }
    };
    if target_order.id != current_trade.target.id {
        return Err(format!(
            "broker close acknowledgement returned order {} instead of target order {}; reconcile with the broker",
            target_order.id, current_trade.target.id
        )
        .into());
    }
    // `Broker::close_trade` acknowledges a market-exit request. Persist that semantic marker even
    // for adapters that return an otherwise unchanged target snapshot; it is the idempotency guard
    // used to prevent a stale caller from issuing the external mutation twice.
    target_order.category = OrderCategory::Market;

    // 5. The broker only acknowledged a request to convert the existing target child into a
    // market exit. Keep the trade open and the stop locally active until sync confirms the fill
    // and the broker-cancelled sibling. Persist the acknowledgement atomically.
    with_savepoint(database, "persist_broker_close_request", |database| {
        ensure_snapshot_unchanged(database, &pending_trade)?;
        database
            .log_write()
            .create_log(log.log.as_str(), &current_trade)?;
        database.order_write().update(&target_order)?;
        let details = serde_json::json!({"market_exit_acknowledged":true,"protective_stop_retained":true});
        audit::record(database, &current_trade, &broker_name, AuditRecord {
            operation_id,
            event_type: "trade.close",
            outcome: "acknowledged",
            prior_status: current_trade.status,
            resulting_status: current_trade.status,
            details: &details,
        })?;
        Ok(())
    })
    .map_err(|error| {
        format!(
            "broker accepted close request for trade {} but local acknowledgement persistence failed; do not resubmit; reconcile with the broker: {error}",
            current_trade.id
        )
    })?;

    Ok((current_trade.balance, log))
}
