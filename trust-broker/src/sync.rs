use apca::api::v2::order::{Order as AlpacaOrder, Status as AlpacaStatus};
use apca::api::v2::orders::{Get, OrdersReq, Status as AlpacaRequestStatus};
use apca::Client;
use trust_model::{Order, OrderStatus, Status, Trade};

use rust_decimal::Decimal;

use chrono::DateTime;
use chrono::NaiveDateTime;
use chrono::Utc;

use std::error::Error;

pub async fn sync_trade(
    client: &Client,
    trade: &Trade,
) -> Result<(Status, Vec<Order>), Box<dyn Error>> {
    let request: OrdersReq = OrdersReq {
        symbols: vec![trade.trading_vehicle.symbol.to_string()],
        status: AlpacaRequestStatus::Closed,
        ..Default::default()
    };

    let orders = client.issue::<Get>(&request).await.unwrap();
    print!("Orders: {:?}", orders);

    // Main entry order
    let entry_order = orders
        .into_iter()
        .find(|x| x.client_order_id.to_string() == trade.id.to_string());

    let entry_order = match entry_order {
        Some(order) => order,
        None => return Err("Entry order not found".into()),
    };

    // Updated orders and trade status
    let mut updated_orders = vec![];
    let mut trade_status = Status::Submitted;

    // Target and stop orders
    for order in entry_order.legs.clone() {
        if order.id.to_string() == trade.target.broker_order_id.unwrap().to_string() {
            let order = map(&order, trade.target.clone());
            if order.status == OrderStatus::Filled {
                // If the target is filled, then the trade status is ClosedTarget.
                trade_status = Status::ClosedTarget;
            }
            updated_orders.push(order);
        } else if order.id.to_string() == trade.safety_stop.broker_order_id.unwrap().to_string() {
            let order = map(&order, trade.safety_stop.clone());
            if order.status == OrderStatus::Filled {
                // If the stop is filled, then the trade status is ClosedStopLoss.
                trade_status = Status::ClosedStopLoss;
            }
            updated_orders.push(order);
        }
    }

    // Updated entry
    let entry_order = map(&entry_order, trade.entry.clone());
    if trade_status == Status::Submitted && entry_order.status == OrderStatus::Filled {
        // If the entry is filled and the target and stop are not, then the trade status is filled.
        trade_status = Status::Filled;
    }
    updated_orders.push(entry_order);

    Ok((trade_status, updated_orders))
}

fn map(alpaca_order: &AlpacaOrder, order: Order) -> Order {
    assert_eq!(
        alpaca_order.id.to_string(),
        order.broker_order_id.unwrap().to_string(),
        "Order IDs do not match"
    );

    let mut order = order;
    order.filled_quantity = alpaca_order.filled_quantity.to_u64().unwrap();
    order.average_filled_price = match alpaca_order.average_fill_price.clone() {
        Some(price) => Some(Decimal::from(price.to_u64().unwrap())),
        None => None,
    };
    order.status = map_status(alpaca_order.status);
    order.filled_at = map_date(alpaca_order.filled_at);
    order.expired_at = map_date(alpaca_order.expired_at);
    order.cancelled_at = map_date(alpaca_order.canceled_at);
    order
}

fn map_date(date: Option<DateTime<Utc>>) -> Option<NaiveDateTime> {
    match date {
        Some(date) => Some(date.naive_utc()),
        None => None,
    }
}

fn map_status(status: AlpacaStatus) -> OrderStatus {
    match status {
        AlpacaStatus::New => OrderStatus::New,
        AlpacaStatus::PartiallyFilled => OrderStatus::PartiallyFilled,
        AlpacaStatus::Filled => OrderStatus::Filled,
        AlpacaStatus::DoneForDay => OrderStatus::DoneForDay,
        AlpacaStatus::Canceled => OrderStatus::Canceled,
        AlpacaStatus::Expired => OrderStatus::Expired,
        AlpacaStatus::Replaced => OrderStatus::Replaced,
        AlpacaStatus::PendingCancel => OrderStatus::PendingCancel,
        AlpacaStatus::PendingReplace => OrderStatus::PendingReplace,
        AlpacaStatus::PendingNew => OrderStatus::PendingNew,
        AlpacaStatus::Accepted => OrderStatus::Accepted,
        AlpacaStatus::Stopped => OrderStatus::Stopped,
        AlpacaStatus::Rejected => OrderStatus::Rejected,
        AlpacaStatus::Suspended => OrderStatus::Suspended,
        AlpacaStatus::Calculated => OrderStatus::Calculated,
        AlpacaStatus::Held => OrderStatus::Held,
        AlpacaStatus::AcceptedForBidding => OrderStatus::AcceptedForBidding,
        AlpacaStatus::Unknown => OrderStatus::Unknown,
    }
}
