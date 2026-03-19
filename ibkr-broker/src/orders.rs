use crate::contracts::{listing_exchange, sec_type_for_vehicle};
use crate::parsing::{
    decimal_field_optional_any, parse_ibkr_datetime, string_field_optional, u64_field_optional_any,
};
use chrono::Utc;
use model::{Order, OrderStatus, Status, TimeInForce, Trade, TradeCategory};
use rust_decimal::Decimal;
use serde_json::{json, Value};
use std::collections::HashSet;
use std::error::Error;

pub(crate) fn build_bracket_orders(
    trade: &Trade,
    account_id: &str,
    conid: &str,
) -> Result<Vec<Value>, Box<dyn Error>> {
    let entry_ref = normalize_order_ref(&trade.entry);
    let target_ref = normalize_order_ref(&trade.target);
    let stop_ref = normalize_order_ref(&trade.safety_stop);
    let listing_exchange = listing_exchange(&trade.trading_vehicle);
    let symbol = trade.trading_vehicle.symbol.to_uppercase();
    let sec_type = sec_type_for_vehicle(&trade.trading_vehicle)?;
    let tif = tif_string(trade.entry.time_in_force);
    let exit_side = exit_side(trade.category);

    Ok(vec![
        json!({
            "acctId": account_id,
            "conid": conid,
            "secType": sec_type,
            "cOID": entry_ref,
            "referrer": "trust-entry",
            "orderType": "LMT",
            "listingExchange": listing_exchange,
            "outsideRTH": trade.entry.extended_hours,
            "price": trade.entry.unit_price.to_string(),
            "side": entry_side(trade.category),
            "ticker": symbol,
            "tif": tif,
            "quantity": trade.entry.quantity,
        }),
        json!({
            "acctId": account_id,
            "conid": conid,
            "secType": sec_type,
            "cOID": target_ref,
            "parentId": normalize_order_ref(&trade.entry),
            "referrer": "trust-target",
            "orderType": "LMT",
            "listingExchange": listing_exchange,
            "outsideRTH": trade.target.extended_hours,
            "price": trade.target.unit_price.to_string(),
            "side": exit_side,
            "ticker": symbol,
            "tif": tif_string(trade.target.time_in_force),
            "quantity": trade.target.quantity,
            "isClose": true,
        }),
        json!({
            "acctId": account_id,
            "conid": conid,
            "secType": sec_type,
            "cOID": stop_ref,
            "parentId": normalize_order_ref(&trade.entry),
            "referrer": "trust-stop",
            "orderType": "STP",
            "listingExchange": listing_exchange,
            "outsideRTH": trade.safety_stop.extended_hours,
            "price": trade.safety_stop.unit_price.to_string(),
            "side": exit_side,
            "ticker": symbol,
            "tif": tif_string(trade.safety_stop.time_in_force),
            "quantity": trade.safety_stop.quantity,
            "isClose": true,
        }),
    ])
}

pub(crate) fn build_close_order(
    trade: &Trade,
    account_id: &str,
    conid: &str,
    close_ref: &str,
) -> Result<Value, Box<dyn Error>> {
    Ok(json!({
        "acctId": account_id,
        "conid": conid,
        "secType": sec_type_for_vehicle(&trade.trading_vehicle)?,
        "cOID": close_ref,
        "referrer": "trust-manual-close",
        "orderType": "MKT",
        "listingExchange": listing_exchange(&trade.trading_vehicle),
        "outsideRTH": trade.target.extended_hours,
        "side": exit_side(trade.category),
        "ticker": trade.trading_vehicle.symbol.to_uppercase(),
        "tif": tif_string(trade.target.time_in_force),
        "quantity": trade.target.quantity,
        "isClose": true,
    }))
}

pub(crate) fn build_modify_order(
    trade: &Trade,
    account_id: &str,
    conid: &str,
    order: &Order,
    new_price: Decimal,
) -> Result<Value, Box<dyn Error>> {
    let (order_type, referrer) = if order.id == trade.target.id {
        ("LMT", "trust-target-modify")
    } else {
        ("STP", "trust-stop-modify")
    };

    Ok(json!({
        "acctId": account_id,
        "conid": conid,
        "secType": sec_type_for_vehicle(&trade.trading_vehicle)?,
        "cOID": normalize_order_ref(order),
        "parentId": normalize_order_ref(&trade.entry),
        "referrer": referrer,
        "orderType": order_type,
        "listingExchange": listing_exchange(&trade.trading_vehicle),
        "outsideRTH": order.extended_hours,
        "price": new_price.to_string(),
        "side": exit_side(trade.category),
        "ticker": trade.trading_vehicle.symbol.to_uppercase(),
        "tif": tif_string(order.time_in_force),
        "quantity": order.quantity,
        "isClose": true,
    }))
}

pub(crate) fn find_live_order_by_ref<'a>(
    orders: &'a [Value],
    order_ref: &str,
) -> Option<&'a Value> {
    orders.iter().find(|order| {
        string_field_optional(order, "order_ref")
            .or_else(|| string_field_optional(order, "local_order_id"))
            .map(|value| value == order_ref)
            .unwrap_or(false)
    })
}

pub(crate) fn map_live_order(base: &Order, live_order: &Value) -> Result<Order, Box<dyn Error>> {
    let mut order = base.clone();
    order.broker_order_id = Some(normalize_order_ref(base));

    let status_text = string_field_optional(live_order, "status")
        .or_else(|| string_field_optional(live_order, "order_status"))
        .unwrap_or_else(|| "unknown".to_string());
    order.status = map_ibkr_order_status(&status_text);

    if let Some(filled_quantity) =
        u64_field_optional_any(live_order, &["filledQuantity", "filled_qty"])
    {
        order.filled_quantity = filled_quantity;
    }
    if let Some(average_filled_price) =
        decimal_field_optional_any(live_order, &["avgPrice", "avg_price"])
    {
        order.average_filled_price = Some(average_filled_price);
    }
    if order.submitted_at.is_none() && order.status != OrderStatus::New {
        order.submitted_at = Some(Utc::now().naive_utc());
    }

    let event_time = string_field_optional(live_order, "lastExecutionTime")
        .as_deref()
        .and_then(parse_ibkr_datetime);
    if order.status == OrderStatus::Filled {
        order.filled_at = event_time
            .or(order.filled_at)
            .or(Some(Utc::now().naive_utc()));
    }
    if order.status == OrderStatus::Canceled {
        order.cancelled_at = event_time
            .or(order.cancelled_at)
            .or(Some(Utc::now().naive_utc()));
    }

    Ok(order)
}

pub(crate) fn map_trade_status(trade: &Trade, updated_orders: &[Order]) -> Status {
    let entry = apply_order_update(&trade.entry, updated_orders);
    let target = apply_order_update(&trade.target, updated_orders);
    let stop = apply_order_update(&trade.safety_stop, updated_orders);

    if stop.status == OrderStatus::Filled {
        return Status::ClosedStopLoss;
    }
    if target.status == OrderStatus::Filled {
        return Status::ClosedTarget;
    }
    if entry.status == OrderStatus::PartiallyFilled {
        return Status::PartiallyFilled;
    }
    if entry.status == OrderStatus::Filled {
        return Status::Filled;
    }
    if entry.status == OrderStatus::Canceled {
        return Status::Canceled;
    }
    if entry.status == OrderStatus::Expired {
        return Status::Expired;
    }
    if entry.status == OrderStatus::Rejected {
        return Status::Rejected;
    }
    if entry.broker_order_id.is_some() || entry.submitted_at.is_some() {
        return Status::Submitted;
    }
    trade.status
}

fn apply_order_update(base: &Order, updates: &[Order]) -> Order {
    updates
        .iter()
        .find(|candidate| candidate.id == base.id)
        .cloned()
        .unwrap_or_else(|| base.clone())
}

pub(crate) fn map_ibkr_order_status(status: &str) -> OrderStatus {
    match status.trim().to_ascii_lowercase().as_str() {
        "submitted" | "pending_submit" | "api_pending" => OrderStatus::New,
        "presubmitted" | "pre_submitted" | "inactive" => OrderStatus::Held,
        "partially_filled" => OrderStatus::PartiallyFilled,
        "filled" => OrderStatus::Filled,
        "cancelled" | "canceled" | "api_cancelled" => OrderStatus::Canceled,
        "pending_cancel" => OrderStatus::PendingCancel,
        "pending_replace" => OrderStatus::PendingReplace,
        "replaced" => OrderStatus::Replaced,
        "rejected" => OrderStatus::Rejected,
        "expired" => OrderStatus::Expired,
        _ => OrderStatus::Unknown,
    }
}

pub(crate) fn entry_side(category: TradeCategory) -> &'static str {
    match category {
        TradeCategory::Long => "BUY",
        TradeCategory::Short => "SELL",
    }
}

pub(crate) fn exit_side(category: TradeCategory) -> &'static str {
    match category {
        TradeCategory::Long => "SELL",
        TradeCategory::Short => "BUY",
    }
}

pub(crate) fn tif_string(time_in_force: TimeInForce) -> &'static str {
    match time_in_force {
        TimeInForce::Day => "DAY",
        TimeInForce::UntilCanceled => "GTC",
        TimeInForce::UntilMarketOpen => "OPG",
        TimeInForce::UntilMarketClose => "CLS",
    }
}

pub(crate) fn normalize_order_ref(order: &Order) -> String {
    order
        .broker_order_id
        .clone()
        .unwrap_or_else(|| order.id.to_string())
}

pub(crate) fn tracked_order_refs(trade: &Trade) -> HashSet<String> {
    [
        normalize_order_ref(&trade.entry),
        normalize_order_ref(&trade.target),
        normalize_order_ref(&trade.safety_stop),
    ]
    .into_iter()
    .collect()
}

#[cfg(test)]
mod tests {
    use super::{find_live_order_by_ref, normalize_order_ref};
    use model::Order;
    use serde_json::json;

    #[test]
    fn normalize_order_ref_prefers_existing_broker_id() {
        let order = Order {
            broker_order_id: Some("ibkr-ref".to_string()),
            ..Order::default()
        };

        assert_eq!(normalize_order_ref(&order), "ibkr-ref");
    }

    #[test]
    fn find_live_order_by_ref_checks_local_order_id_fallback() {
        let orders = vec![json!({
            "local_order_id": "entry-ref",
            "status": "Submitted"
        })];

        let found = find_live_order_by_ref(&orders, "entry-ref").expect("order");

        assert_eq!(found["local_order_id"], json!("entry-ref"));
    }
}
