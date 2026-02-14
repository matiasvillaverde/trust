use crate::commands;
use crate::commands::trade;
use model::{Account, Broker, BrokerEvent, DatabaseFactory, OrderStatus, Status, Trade, WatchControl, WatchEvent, WatchOptions};
use std::error::Error;
use uuid::Uuid;
use serde_json::Value as JsonValue;

fn derive_broker_status_from_trade(trade: &Trade) -> Option<Status> {
    // Exit fills have priority.
    if trade.safety_stop.status == OrderStatus::Filled {
        return Some(Status::ClosedStopLoss);
    }
    if trade.target.status == OrderStatus::Filled {
        return Some(Status::ClosedTarget);
    }

    if trade.entry.status == OrderStatus::Filled {
        return Some(Status::Filled);
    }

    // If entry terminated without fill, propagate to trade.
    match trade.entry.status {
        OrderStatus::Canceled => Some(Status::Canceled),
        OrderStatus::Expired => Some(Status::Expired),
        OrderStatus::Rejected => Some(Status::Rejected),
        _ => None,
    }
}

fn cancel_submitted_due_to_broker(
    trade: &Trade,
    new_status: Status,
    database: &mut dyn DatabaseFactory,
) -> Result<Trade, Box<dyn Error>> {
    // Update trade status first.
    let trade = database
        .trade_write()
        .update_trade_status(new_status, trade)?;

    // Return any out-of-market funds back to account.
    let (_tx, _account_balance, _trade_balance) =
        commands::transaction::transfer_to_account_from(&trade, database)?;

    Ok(trade)
}

/// Watch a trade and persist broker events + apply state transitions in the database.
///
/// This uses broker websocket updates (when supported) with periodic reconciliation per `options`.
pub fn watch_trade(
    trade: &Trade,
    account: &Account,
    database: &mut dyn DatabaseFactory,
    broker: &dyn Broker,
    options: WatchOptions,
    mut on_tick: impl FnMut(&Trade, &WatchEvent) -> Result<WatchControl, Box<dyn Error>>,
) -> Result<(), Box<dyn Error>> {
    // Security/sanity.
    if trade.account_id != account.id {
        return Err("Trade does not belong to account".into());
    }

    let trade_id = trade.id;

    let mut callback = |evt: WatchEvent| -> Result<WatchControl, Box<dyn Error>> {
        // Security: prevent unbounded growth (even though Alpaca payloads are small).
        let payload_json = if evt.payload_json.len() > 1_000_000 {
            "{\"error\":\"payload_too_large\"}".to_string()
        } else if serde_json::from_str::<JsonValue>(&evt.payload_json).is_err() {
            "{\"error\":\"payload_invalid_json\"}".to_string()
        } else {
            evt.payload_json.clone()
        };

        // 1. Persist raw broker event for audit/replay.
        let broker_event = BrokerEvent {
            id: Uuid::new_v4(),
            created_at: chrono::Utc::now().naive_utc(),
            updated_at: chrono::Utc::now().naive_utc(),
            deleted_at: None,
            account_id: account.id,
            trade_id,
            source: evt.broker_source.clone(),
            stream: evt.broker_stream.clone(),
            event_type: evt.broker_event_type.clone(),
            broker_order_id: evt.broker_order_id,
            payload_json,
        };
        let _ = database.broker_event_write().create_event(&broker_event)?;

        // 2. Apply updated orders to DB.
        {
            let mut order_write = database.order_write();
            for order in &evt.updated_orders {
                order_write.update(order)?;
            }
        }

        // 3. Re-read trade with updated orders.
        let trade_after_orders = database.trade_read().read_trade(trade_id)?;

        // 4. Apply trade-level transitions and accounting when applicable.
        if let Some(next_status) = derive_broker_status_from_trade(&trade_after_orders) {
            match next_status {
                Status::Filled | Status::ClosedStopLoss | Status::ClosedTarget => {
                    let _ = trade::update_status(&trade_after_orders, next_status, database)?;
                }
                Status::Canceled | Status::Expired | Status::Rejected => {
                    // If the entry never filled, unwind trade funding back to account.
                    if trade_after_orders.status == Status::Submitted {
                        let _ = cancel_submitted_due_to_broker(&trade_after_orders, next_status, database)?;
                    } else {
                        // If we were already filled, do not allow entry-cancel to overwrite terminal states.
                    }
                }
                _ => {}
            }
        }

        // 5. Refresh derived account overview (cheap, consistent).
        let trade_latest = database.trade_read().read_trade(trade_id)?;
        commands::balance::calculate_account(database, account, &trade_latest.currency)?;

        // 6. Emit tick to caller.
        let control = on_tick(&trade_latest, &evt)?;

        Ok(control)
    };

    broker.watch_trade(trade, account, options, &mut callback)
}
