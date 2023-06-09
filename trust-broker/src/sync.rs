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

    // Main entry order
    let entry_order = orders
        .into_iter()
        .find(|x| x.client_order_id == trade.id.to_string());

    let entry_order = match entry_order {
        Some(order) => order,
        None => return Err("Entry order not found".into()),
    };
    map_orders(entry_order, trade)
}

// TODO: Test this
fn map_orders(
    entry_order: AlpacaOrder,
    trade: &Trade,
) -> Result<(Status, Vec<Order>), Box<dyn Error>> {
    // Updated orders and trade status
    let mut updated_orders = vec![];
    let mut trade_status = Status::Submitted;

    // Target and stop orders
    for order in entry_order.legs.clone() {
        if order.id.to_string() == trade.target.broker_order_id.unwrap().to_string() {
            // TODO: It seems that Ids from Target and StopLoss are mixed
            let order = map_order(&order, trade.target.clone());
            if order.status == OrderStatus::Filled {
                // If the target is filled, then the trade status is ClosedTarget.
                trade_status = Status::ClosedTarget;
            }
            if order != trade.target {
                // If the target is updated, then we add it to the updated orders.
                updated_orders.push(order);
            }
        } else if order.id.to_string() == trade.safety_stop.broker_order_id.unwrap().to_string() {
            let order = map_order(&order, trade.safety_stop.clone());
            if order.status == OrderStatus::Filled {
                // If the stop is filled, then the trade status is ClosedStopLoss.
                trade_status = Status::ClosedStopLoss;
            }
            if order != trade.safety_stop {
                // If the stop is updated, then we add it to the updated orders.
                updated_orders.push(order);
            }
        }
    }

    // Updated entry
    let entry_order = map_order(&entry_order, trade.entry.clone());
    if trade_status == Status::Submitted && entry_order.status == OrderStatus::Filled {
        // If the entry is filled and the target and stop are not, then the trade status is filled.
        trade_status = Status::Filled;
    }
    if entry_order != trade.entry {
        // If the entry is updated, then we add it to the updated orders.
        updated_orders.push(entry_order);
    }

    Ok((trade_status, updated_orders))
}

fn map_order(alpaca_order: &AlpacaOrder, order: Order) -> Order {
    assert_eq!(
        alpaca_order.id.to_string(),
        order.broker_order_id.unwrap().to_string(),
        "Order IDs do not match"
    );

    let mut order = order;
    order.filled_quantity = alpaca_order.filled_quantity.to_u64().unwrap();
    order.average_filled_price = alpaca_order
        .average_fill_price
        .clone()
        .map(|price| Decimal::from(price.to_u64().unwrap()));
    order.status = map_status(alpaca_order.status);
    order.filled_at = map_date(alpaca_order.filled_at);
    order.expired_at = map_date(alpaca_order.expired_at);
    order.cancelled_at = map_date(alpaca_order.canceled_at);
    order
}

fn map_date(date: Option<DateTime<Utc>>) -> Option<NaiveDateTime> {
    date.map(|date| date.naive_utc())
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

#[cfg(test)]
mod tests {
    use super::*;
    use apca::api::v2::order::{Amount, Class, Side, TimeInForce, Type};
    use apca::api::v2::{asset, order::Id};
    use chrono::NaiveDateTime;
    use num_decimal::Num;
    use rust_decimal_macros::dec;
    use uuid::Uuid;

    fn default() -> AlpacaOrder {
        AlpacaOrder {
            id: Id(Uuid::parse_str("00000000-0000-0000-0000-000000000000").unwrap()),
            client_order_id: "".to_owned(),
            status: AlpacaStatus::New,
            created_at: Utc::now(),
            updated_at: None,
            submitted_at: None,
            filled_at: None,
            expired_at: None,
            canceled_at: None,
            asset_class: asset::Class::default(),
            asset_id: asset::Id(
                Uuid::parse_str("00000000-0000-0000-0000-000000000000")
                    .unwrap()
                    .to_owned(),
            ),
            symbol: "".to_owned(),
            amount: Amount::quantity(10),
            filled_quantity: Num::default(),
            type_: Type::default(),
            class: Class::default(),
            side: Side::Buy,
            time_in_force: TimeInForce::default(),
            limit_price: None,
            stop_price: None,
            trail_price: None,
            trail_percent: None,
            average_fill_price: None,
            legs: vec![],
            extended_hours: false,
        }
    }

    #[test]
    fn test_map_orders_entry_not_filled() {
        // Create a sample AlpacaOrder and Trade
        let alpaca_order = default();

        let trade = Trade {
            target: Order {
                ..Default::default()
            },
            safety_stop: Order {
                ..Default::default()
            },
            entry: Order {
                broker_order_id: Some(
                    Uuid::parse_str("00000000-0000-0000-0000-000000000000").unwrap(),
                ),
                ..Default::default()
            },
            ..Default::default()
        };

        // Call the map_orders function
        let result = map_orders(alpaca_order, &trade).unwrap();

        // Check that the trade status is Filled and there is only one updated order
        assert_eq!(result.0, Status::Submitted);
        assert_eq!(result.1.len(), 0);
    }

    #[test]
    fn test_map_order_ids_match() {
        let alpaca_order = default();
        let order = Order {
            broker_order_id: Some(Uuid::parse_str("00000000-0000-0000-0000-000000000000").unwrap()),
            ..Default::default()
        };
        let mapped_order = map_order(&alpaca_order, order);

        assert_eq!(
            mapped_order.broker_order_id.unwrap(),
            Uuid::parse_str("00000000-0000-0000-0000-000000000000").unwrap()
        );
    }

    #[test]
    fn test_map_filled_quantity() {
        let mut alpaca_order = default();
        alpaca_order.filled_quantity = Num::from(10);

        let order = Order {
            broker_order_id: Some(Uuid::parse_str("00000000-0000-0000-0000-000000000000").unwrap()),
            ..Default::default()
        };

        let mapped_order = map_order(&alpaca_order, order);

        assert_eq!(mapped_order.filled_quantity, 10);
    }

    #[test]
    fn test_map_average_filled_price() {
        let mut alpaca_order = default();
        alpaca_order.average_fill_price = Some(Num::from(2112));

        let order = Order {
            broker_order_id: Some(Uuid::parse_str("00000000-0000-0000-0000-000000000000").unwrap()),
            ..Default::default()
        };

        let mapped_order = map_order(&alpaca_order, order);

        assert_eq!(mapped_order.average_filled_price.unwrap(), dec!(2112));
    }

    #[test]
    fn test_map_order_status() {
        let mut alpaca_order = default();
        alpaca_order.status = AlpacaStatus::Filled;

        let order = Order {
            broker_order_id: Some(Uuid::parse_str("00000000-0000-0000-0000-000000000000").unwrap()),
            ..Default::default()
        };

        let mapped_order = map_order(&alpaca_order, order);

        assert_eq!(mapped_order.status, OrderStatus::Filled);
    }

    #[test]
    fn test_map_filled_at() {
        let now = Utc::now();
        let mut alpaca_order = default();
        alpaca_order.filled_at = Some(now);

        let order = Order {
            broker_order_id: Some(Uuid::parse_str("00000000-0000-0000-0000-000000000000").unwrap()),
            ..Default::default()
        };
        let mapped_order = map_order(&alpaca_order, order);
        assert_eq!(mapped_order.filled_at, map_date(Some(now)));
    }

    #[test]
    fn test_map_expired_at() {
        let now = Utc::now();
        let mut alpaca_order = default();
        alpaca_order.expired_at = Some(now);

        let order = Order {
            broker_order_id: Some(Uuid::parse_str("00000000-0000-0000-0000-000000000000").unwrap()),
            ..Default::default()
        };

        let mapped_order = map_order(&alpaca_order, order);

        assert_eq!(mapped_order.expired_at, map_date(Some(now)));
    }

    #[test]
    fn test_map_cancelled_at() {
        let now = Utc::now();
        let mut alpaca_order = default();
        alpaca_order.canceled_at = Some(now);

        let order = Order {
            broker_order_id: Some(Uuid::parse_str("00000000-0000-0000-0000-000000000000").unwrap()),
            ..Default::default()
        };

        let mapped_order = map_order(&alpaca_order, order);

        assert_eq!(mapped_order.cancelled_at, map_date(Some(now)));
    }
    #[test]
    fn test_map_date_with_none() {
        let expected: Option<NaiveDateTime> = None;
        assert_eq!(map_date(None), expected);
    }

    #[test]
    fn test_map_status() {
        assert_eq!(map_status(AlpacaStatus::New), OrderStatus::New);
        assert_eq!(
            map_status(AlpacaStatus::PartiallyFilled),
            OrderStatus::PartiallyFilled
        );
        assert_eq!(map_status(AlpacaStatus::Filled), OrderStatus::Filled);
        assert_eq!(
            map_status(AlpacaStatus::DoneForDay),
            OrderStatus::DoneForDay
        );
        assert_eq!(map_status(AlpacaStatus::Canceled), OrderStatus::Canceled);
        assert_eq!(map_status(AlpacaStatus::Expired), OrderStatus::Expired);
        assert_eq!(map_status(AlpacaStatus::Replaced), OrderStatus::Replaced);
        assert_eq!(
            map_status(AlpacaStatus::PendingCancel),
            OrderStatus::PendingCancel
        );
        assert_eq!(
            map_status(AlpacaStatus::PendingReplace),
            OrderStatus::PendingReplace
        );
        assert_eq!(
            map_status(AlpacaStatus::PendingNew),
            OrderStatus::PendingNew
        );
        assert_eq!(map_status(AlpacaStatus::Accepted), OrderStatus::Accepted);
        assert_eq!(map_status(AlpacaStatus::Stopped), OrderStatus::Stopped);
        assert_eq!(map_status(AlpacaStatus::Rejected), OrderStatus::Rejected);
        assert_eq!(map_status(AlpacaStatus::Suspended), OrderStatus::Suspended);
        assert_eq!(
            map_status(AlpacaStatus::Calculated),
            OrderStatus::Calculated
        );
        assert_eq!(map_status(AlpacaStatus::Held), OrderStatus::Held);
        assert_eq!(
            map_status(AlpacaStatus::AcceptedForBidding),
            OrderStatus::AcceptedForBidding
        );
        assert_eq!(map_status(AlpacaStatus::Unknown), OrderStatus::Unknown);
    }
}
