use apca::api::v2::order::{Order as AlpacaOrder, Status as AlpacaStatus};
use chrono::DateTime;
use chrono::NaiveDateTime;
use chrono::Utc;
use rust_decimal::Decimal;
use std::error::Error;
use std::str::FromStr;
use trust_model::{Order, OrderCategory, OrderStatus, Status, Trade};
use uuid::Uuid;

/// Maps an Alpaca order to our domain model.
pub fn map_entry(alpaca_order: AlpacaOrder, trade: &Trade) -> Result<Vec<Order>, Box<dyn Error>> {
    // 1. Updated orders and trade status
    let mut updated_orders = vec![];

    // 2. Target and stop orders
    updated_orders.extend(alpaca_order.legs.iter().filter_map(|order| {
        match order.id.to_string().as_str() {
            id if id == trade.target.broker_order_id.unwrap().to_string() => {
                // 1. Map target order to our domain model.
                let order = map(order, trade.target.clone());

                // 2. If the target is updated, then we add it to the updated orders.
                if order != trade.target {
                    Some(order)
                } else {
                    None
                }
            }
            id if id == trade.safety_stop.broker_order_id.unwrap().to_string() => {
                // 1. Map stop order to our domain model.
                let order = map(order, trade.safety_stop.clone());

                // 2. If the stop is updated, then we add it to the updated orders.
                if order != trade.safety_stop {
                    Some(order)
                } else {
                    None
                }
            }
            _ => None,
        }
    }));

    // 3. Map entry order to our domain model.
    let entry_order = map(&alpaca_order, trade.entry.clone());

    // 4. If the entry is updated, then we add it to the updated orders.
    if entry_order != trade.entry {
        updated_orders.push(entry_order);
    }

    Ok(updated_orders)
}

pub fn map_target(alpaca_order: AlpacaOrder, trade: &Trade) -> Result<Vec<Order>, Box<dyn Error>> {
    Ok(vec![map(&alpaca_order, trade.target.clone())])
}

pub fn map_trade_status(trade: &Trade, updated_orders: &[Order]) -> Status {
    if updated_orders
        .iter()
        .any(|order| order.status == OrderStatus::Filled && order.id == trade.target.id)
    {
        return Status::ClosedTarget;
    }

    if updated_orders
        .iter()
        .any(|order| order.status == OrderStatus::Filled && order.id == trade.safety_stop.id)
    {
        return Status::ClosedStopLoss;
    }

    if updated_orders
        .iter()
        .any(|order| order.status == OrderStatus::Filled && order.id == trade.entry.id)
    {
        return Status::Filled;
    }

    trade.status
}

fn map(alpaca_order: &AlpacaOrder, order: Order) -> Order {
    assert_eq!(
        alpaca_order.id.to_string(),
        order
            .broker_order_id
            .expect("order does not have a broker id. It can not be mapped into an alpaca order")
            .to_string(),
        "Order IDs do not match"
    );

    let mut order = order;
    order.filled_quantity = alpaca_order.filled_quantity.to_u64().unwrap();
    order.average_filled_price = alpaca_order
        .average_fill_price
        .clone()
        .map(|price| Decimal::from_str(price.to_string().as_str()).unwrap());
    order.status = map_from_alpaca(alpaca_order.status);
    order.filled_at = map_date(alpaca_order.filled_at);
    order.expired_at = map_date(alpaca_order.expired_at);
    order.cancelled_at = map_date(alpaca_order.canceled_at);
    order
}

pub fn map_close_order(alpaca_order: &AlpacaOrder, target: Order) -> Order {
    let mut order = target;
    order.broker_order_id = Some(Uuid::parse_str(&alpaca_order.id.to_string()).unwrap());
    order.status = map_from_alpaca(alpaca_order.status);
    order.submitted_at = map_date(alpaca_order.submitted_at);
    order.category = OrderCategory::Market;
    order
}

fn map_date(date: Option<DateTime<Utc>>) -> Option<NaiveDateTime> {
    date.map(|date| date.naive_utc())
}

fn map_from_alpaca(status: AlpacaStatus) -> OrderStatus {
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
    fn test_map_orders_nothing_to_map() {
        let alpaca_order = default();
        let trade = Trade {
            entry: Order {
                broker_order_id: Some(
                    Uuid::parse_str("00000000-0000-0000-0000-000000000000").unwrap(),
                ),
                ..Default::default()
            },
            ..Default::default()
        };
        let err = map_entry(alpaca_order, &trade).unwrap();
        assert_eq!(err.len(), 0);
    }

    #[test]
    #[should_panic(
        expected = "order does not have a broker id. It can not be mapped into an alpaca order"
    )]
    fn test_map_orders_entry_id_are_different() {
        // Create a sample AlpacaOrder and Trade
        let alpaca_order = default();
        let trade = Trade::default();
        _ = map_entry(alpaca_order, &trade);
    }

    #[test]
    fn test_map_orders_returns_entry() {
        let entry_id = Uuid::new_v4();

        // Create a sample AlpacaOrder and Trade
        let alpaca_order = AlpacaOrder {
            id: Id(entry_id),
            filled_at: Some(Utc::now()),
            filled_quantity: Num::from(100),
            status: AlpacaStatus::Filled,
            average_fill_price: Some(Num::from(10)),
            ..default()
        };

        let trade = Trade {
            entry: Order {
                broker_order_id: Some(entry_id),
                ..Default::default()
            },
            ..Default::default()
        };

        let result = map_entry(alpaca_order, &trade).unwrap();

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].status, OrderStatus::Filled);
        assert!(result[0].filled_at.is_some());
        assert_eq!(result[0].filled_quantity, 100);
        assert_eq!(result[0].average_filled_price, Some(dec!(10)));
    }

    #[test]
    fn test_map_orders_returns_entry_and_target() {
        let entry_id = Uuid::new_v4();
        let target_id = Uuid::new_v4();

        // Create a sample AlpacaOrder and Trade
        let alpaca_order = AlpacaOrder {
            id: Id(entry_id),
            filled_at: Some(Utc::now()),
            filled_quantity: Num::from(100),
            status: AlpacaStatus::Filled,
            average_fill_price: Some(Num::from(10)),
            legs: vec![AlpacaOrder {
                id: Id(target_id),
                filled_at: Some(Utc::now()),
                filled_quantity: Num::from(100),
                status: AlpacaStatus::Filled,
                average_fill_price: Some(Num::from(11)),
                ..default()
            }],
            ..default()
        };

        let trade = Trade {
            target: Order {
                broker_order_id: Some(target_id),
                ..Default::default()
            },
            safety_stop: Order {
                broker_order_id: Some(Uuid::new_v4()),
                ..Default::default()
            },
            entry: Order {
                broker_order_id: Some(entry_id),
                ..Default::default()
            },
            ..Default::default()
        };

        let result = map_entry(alpaca_order, &trade).unwrap();

        assert_eq!(result.len(), 2);

        // Entry
        assert_eq!(result[0].status, OrderStatus::Filled);
        assert!(result[0].filled_at.is_some());
        assert_eq!(result[0].filled_quantity, 100);
        assert_eq!(result[0].average_filled_price, Some(dec!(11)));

        // Target
        assert_eq!(result[1].status, OrderStatus::Filled);
        assert!(result[1].filled_at.is_some());
        assert_eq!(result[1].filled_quantity, 100);
        assert_eq!(result[1].average_filled_price, Some(dec!(10)));
    }

    #[test]
    fn test_map_orders_returns_entry_and_stop() {
        let entry_id = Uuid::new_v4();
        let stop_id = Uuid::new_v4();

        // Create a sample AlpacaOrder and Trade
        let alpaca_order = AlpacaOrder {
            id: Id(entry_id),
            filled_at: Some(Utc::now()),
            filled_quantity: Num::from(100),
            status: AlpacaStatus::Filled,
            average_fill_price: Some(Num::from(10)),
            legs: vec![AlpacaOrder {
                id: Id(stop_id),
                filled_at: Some(Utc::now()),
                filled_quantity: Num::from(100),
                status: AlpacaStatus::Filled,
                average_fill_price: Some(Num::from(9)),
                ..default()
            }],
            ..default()
        };

        let trade = Trade {
            target: Order {
                broker_order_id: Some(Uuid::new_v4()),
                ..Default::default()
            },
            safety_stop: Order {
                broker_order_id: Some(stop_id),
                ..Default::default()
            },
            entry: Order {
                broker_order_id: Some(entry_id),
                ..Default::default()
            },
            ..Default::default()
        };

        let result = map_entry(alpaca_order, &trade).unwrap();

        assert_eq!(result.len(), 2);

        // Entry
        assert_eq!(result[0].status, OrderStatus::Filled);
        assert!(result[0].filled_at.is_some());
        assert_eq!(result[0].filled_quantity, 100);
        assert_eq!(result[0].average_filled_price, Some(dec!(9)));

        // Stop
        assert_eq!(result[1].status, OrderStatus::Filled);
        assert!(result[1].filled_at.is_some());
        assert_eq!(result[1].filled_quantity, 100);
        assert_eq!(result[1].average_filled_price, Some(dec!(10)));
    }

    #[test]
    fn test_map_status_submitted() {
        let target_id = Uuid::new_v4();
        let safety_stop_id = Uuid::new_v4();
        let entry_id = Uuid::new_v4();

        let trade = Trade {
            target: Order {
                id: target_id,
                status: OrderStatus::Filled,
                ..Default::default()
            },
            safety_stop: Order {
                id: safety_stop_id,
                status: OrderStatus::New,
                ..Default::default()
            },
            entry: Order {
                id: entry_id,
                status: OrderStatus::Filled,
                ..Default::default()
            },
            status: Status::Submitted,
            ..Default::default()
        };
        let updated_orders = vec![
            Order {
                id: entry_id,
                status: OrderStatus::New,
                ..Default::default()
            },
            Order {
                id: safety_stop_id,
                status: OrderStatus::New,
                ..Default::default()
            },
        ];

        assert_eq!(map_trade_status(&trade, &updated_orders), Status::Submitted);
    }

    #[test]
    fn test_map_status_filled_entry() {
        let target_id = Uuid::new_v4();
        let safety_stop_id = Uuid::new_v4();
        let entry_id = Uuid::new_v4();

        let trade = Trade {
            target: Order {
                id: target_id,
                status: OrderStatus::New,
                ..Default::default()
            },
            safety_stop: Order {
                id: safety_stop_id,
                status: OrderStatus::New,
                ..Default::default()
            },
            entry: Order {
                id: entry_id,
                status: OrderStatus::Filled,
                ..Default::default()
            },
            status: Status::Submitted,
            ..Default::default()
        };
        let updated_orders = vec![Order {
            id: entry_id,
            status: OrderStatus::Filled,
            ..Default::default()
        }];

        assert_eq!(map_trade_status(&trade, &updated_orders), Status::Filled);
    }

    #[test]
    fn test_map_status_filled_target() {
        let target_id = Uuid::new_v4();
        let safety_stop_id = Uuid::new_v4();
        let entry_id = Uuid::new_v4();

        let trade = Trade {
            target: Order {
                id: target_id,
                status: OrderStatus::Filled,
                ..Default::default()
            },
            safety_stop: Order {
                id: safety_stop_id,
                status: OrderStatus::New,
                ..Default::default()
            },
            entry: Order {
                id: entry_id,
                status: OrderStatus::Filled,
                ..Default::default()
            },
            status: Status::Submitted,
            ..Default::default()
        };
        let updated_orders = vec![
            Order {
                id: entry_id,
                status: OrderStatus::Filled,
                ..Default::default()
            },
            Order {
                id: target_id,
                status: OrderStatus::Filled,
                ..Default::default()
            },
        ];

        assert_eq!(
            map_trade_status(&trade, &updated_orders),
            Status::ClosedTarget
        );
    }

    #[test]
    fn test_map_status_filled_only_target() {
        let target_id = Uuid::new_v4();
        let safety_stop_id = Uuid::new_v4();
        let entry_id = Uuid::new_v4();

        let trade = Trade {
            target: Order {
                id: target_id,
                status: OrderStatus::Filled,
                ..Default::default()
            },
            safety_stop: Order {
                id: safety_stop_id,
                status: OrderStatus::New,
                ..Default::default()
            },
            entry: Order {
                id: entry_id,
                status: OrderStatus::Filled,
                ..Default::default()
            },
            status: Status::Submitted,
            ..Default::default()
        };
        let updated_orders = vec![Order {
            id: target_id,
            status: OrderStatus::Filled,
            ..Default::default()
        }];

        assert_eq!(
            map_trade_status(&trade, &updated_orders),
            Status::ClosedTarget
        );
    }

    #[test]
    fn test_map_status_filled_stop() {
        let target_id = Uuid::new_v4();
        let safety_stop_id = Uuid::new_v4();
        let entry_id = Uuid::new_v4();

        let trade = Trade {
            target: Order {
                id: target_id,
                status: OrderStatus::Filled,
                ..Default::default()
            },
            safety_stop: Order {
                id: safety_stop_id,
                status: OrderStatus::New,
                ..Default::default()
            },
            entry: Order {
                id: entry_id,
                status: OrderStatus::Filled,
                ..Default::default()
            },
            status: Status::Submitted,
            ..Default::default()
        };
        let updated_orders = vec![
            Order {
                id: entry_id,
                status: OrderStatus::Filled,
                ..Default::default()
            },
            Order {
                id: safety_stop_id,
                status: OrderStatus::Filled,
                ..Default::default()
            },
        ];

        assert_eq!(
            map_trade_status(&trade, &updated_orders),
            Status::ClosedStopLoss
        );
    }

    #[test]
    fn test_map_order_ids_match() {
        let alpaca_order = default();
        let order = Order {
            broker_order_id: Some(Uuid::parse_str("00000000-0000-0000-0000-000000000000").unwrap()),
            ..Default::default()
        };
        let mapped_order = map(&alpaca_order, order);

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

        let mapped_order = map(&alpaca_order, order);

        assert_eq!(mapped_order.filled_quantity, 10);
    }

    #[test]
    fn test_map_average_filled_price() {
        let mut alpaca_order = default();
        alpaca_order.average_fill_price = Some(Num::from_str("2112.1212").unwrap());

        let order = Order {
            broker_order_id: Some(Uuid::parse_str("00000000-0000-0000-0000-000000000000").unwrap()),
            ..Default::default()
        };

        let mapped_order = map(&alpaca_order, order);

        assert_eq!(mapped_order.average_filled_price.unwrap(), dec!(2112.1212));
    }

    #[test]
    fn test_map_order_status() {
        let mut alpaca_order = default();
        alpaca_order.status = AlpacaStatus::Filled;

        let order = Order {
            broker_order_id: Some(Uuid::parse_str("00000000-0000-0000-0000-000000000000").unwrap()),
            ..Default::default()
        };

        let mapped_order = map(&alpaca_order, order);

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
        let mapped_order = map(&alpaca_order, order);
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

        let mapped_order = map(&alpaca_order, order);

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

        let mapped_order = map(&alpaca_order, order);

        assert_eq!(mapped_order.cancelled_at, map_date(Some(now)));
    }
    #[test]
    fn test_map_date_with_none() {
        let expected: Option<NaiveDateTime> = None;
        assert_eq!(map_date(None), expected);
    }

    #[test]
    fn test_map_from_alpaca() {
        assert_eq!(map_from_alpaca(AlpacaStatus::New), OrderStatus::New);
        assert_eq!(
            map_from_alpaca(AlpacaStatus::PartiallyFilled),
            OrderStatus::PartiallyFilled
        );
        assert_eq!(map_from_alpaca(AlpacaStatus::Filled), OrderStatus::Filled);
        assert_eq!(
            map_from_alpaca(AlpacaStatus::DoneForDay),
            OrderStatus::DoneForDay
        );
        assert_eq!(
            map_from_alpaca(AlpacaStatus::Canceled),
            OrderStatus::Canceled
        );
        assert_eq!(map_from_alpaca(AlpacaStatus::Expired), OrderStatus::Expired);
        assert_eq!(
            map_from_alpaca(AlpacaStatus::Replaced),
            OrderStatus::Replaced
        );
        assert_eq!(
            map_from_alpaca(AlpacaStatus::PendingCancel),
            OrderStatus::PendingCancel
        );
        assert_eq!(
            map_from_alpaca(AlpacaStatus::PendingReplace),
            OrderStatus::PendingReplace
        );
        assert_eq!(
            map_from_alpaca(AlpacaStatus::PendingNew),
            OrderStatus::PendingNew
        );
        assert_eq!(
            map_from_alpaca(AlpacaStatus::Accepted),
            OrderStatus::Accepted
        );
        assert_eq!(map_from_alpaca(AlpacaStatus::Stopped), OrderStatus::Stopped);
        assert_eq!(
            map_from_alpaca(AlpacaStatus::Rejected),
            OrderStatus::Rejected
        );
        assert_eq!(
            map_from_alpaca(AlpacaStatus::Suspended),
            OrderStatus::Suspended
        );
        assert_eq!(
            map_from_alpaca(AlpacaStatus::Calculated),
            OrderStatus::Calculated
        );
        assert_eq!(map_from_alpaca(AlpacaStatus::Held), OrderStatus::Held);
        assert_eq!(
            map_from_alpaca(AlpacaStatus::AcceptedForBidding),
            OrderStatus::AcceptedForBidding
        );
        assert_eq!(map_from_alpaca(AlpacaStatus::Unknown), OrderStatus::Unknown);
    }
}
