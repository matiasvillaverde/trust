use apca::api::v2::order::{Order as AlpacaOrder, Status as AlpacaStatus};
use chrono::DateTime;
use chrono::NaiveDateTime;
use chrono::Utc;
use model::{Order, OrderCategory, OrderStatus, Status, Trade};
use rust_decimal::Decimal;
use std::error::Error;
use std::str::FromStr;
use uuid::Uuid;

/// Maps an Alpaca order to our domain model.
pub fn map_entry(alpaca_order: AlpacaOrder, trade: &Trade) -> Result<Vec<Order>, Box<dyn Error>> {
    // 1. Updated orders and trade status
    let mut updated_orders = vec![];

    // 2. Target and stop orders
    updated_orders.extend(alpaca_order.legs.iter().filter_map(|order| {
        let order_id_str = order.id.to_string();

        // Safely handle target order mapping
        if let Some(target_broker_id) = trade.target.broker_order_id {
            if order_id_str == target_broker_id.to_string() {
                // 1. Map target order to our domain model.
                return match map(order, trade.target.clone()) {
                    Ok(mapped_order) => {
                        // 2. If the target is updated, then we add it to the updated orders.
                        if mapped_order != trade.target {
                            Some(mapped_order)
                        } else {
                            None
                        }
                    }
                    Err(e) => {
                        eprintln!("Error mapping target order: {}", e);
                        None
                    }
                };
            }
        }

        // Safely handle safety stop order mapping
        if let Some(stop_broker_id) = trade.safety_stop.broker_order_id {
            if order_id_str == stop_broker_id.to_string() {
                // 1. Map stop order to our domain model.
                return match map(order, trade.safety_stop.clone()) {
                    Ok(mapped_order) => {
                        // 2. If the stop is updated, then we add it to the updated orders.
                        if mapped_order != trade.safety_stop {
                            Some(mapped_order)
                        } else {
                            None
                        }
                    }
                    Err(e) => {
                        eprintln!("Error mapping safety stop order: {}", e);
                        None
                    }
                };
            }
        }

        None
    }));

    // 3. Map entry order to our domain model.
    let entry_order = map(&alpaca_order, trade.entry.clone())?;

    // 4. If the entry is updated, then we add it to the updated orders.
    if entry_order != trade.entry {
        updated_orders.push(entry_order);
    }

    Ok(updated_orders)
}

pub fn map_target(alpaca_order: AlpacaOrder, trade: &Trade) -> Result<Vec<Order>, Box<dyn Error>> {
    Ok(vec![map(&alpaca_order, trade.target.clone())?])
}

// Alternative approach using helper functions for cleaner code

fn apply_updates_to_order(original: &Order, updates: &[Order]) -> Order {
    if let Some(updated) = updates.iter().find(|o| o.id == original.id) {
        updated.clone()
    } else {
        original.clone()
    }
}

fn has_recent_fill(order_id: Uuid, updated_orders: &[Order]) -> bool {
    updated_orders
        .iter()
        .any(|order| order.id == order_id && order.status == OrderStatus::Filled)
}

fn has_recent_unfill(order_id: Uuid, updated_orders: &[Order]) -> bool {
    updated_orders
        .iter()
        .any(|order| order.id == order_id && order.status != OrderStatus::Filled)
}

pub fn map_trade_status(trade: &Trade, updated_orders: &[Order]) -> Status {
    // Priority 1: Recent fills (what became filled in this sync)
    if has_recent_fill(trade.safety_stop.id, updated_orders) {
        return Status::ClosedStopLoss;
    }

    if has_recent_fill(trade.target.id, updated_orders) {
        return Status::ClosedTarget;
    }

    if has_recent_fill(trade.entry.id, updated_orders) {
        return Status::Filled;
    }

    // Priority 2: Recent unfills (orders that became not filled)
    if has_recent_unfill(trade.entry.id, updated_orders) {
        return Status::Submitted;
    }

    // Priority 3: Overall state (for orders already filled from previous syncs)
    let current_safety_stop = apply_updates_to_order(&trade.safety_stop, updated_orders);
    let current_target = apply_updates_to_order(&trade.target, updated_orders);
    let current_entry = apply_updates_to_order(&trade.entry, updated_orders);

    if current_safety_stop.status == OrderStatus::Filled {
        return Status::ClosedStopLoss;
    }

    if current_target.status == OrderStatus::Filled {
        return Status::ClosedTarget;
    }

    if current_entry.status == OrderStatus::Filled {
        return Status::Filled;
    }

    trade.status
}

fn map(alpaca_order: &AlpacaOrder, order: Order) -> Result<Order, Box<dyn Error>> {
    let broker_order_id = order
        .broker_order_id
        .ok_or("order does not have a broker id. It can not be mapped into an alpaca order")?;

    if alpaca_order.id.to_string() != broker_order_id.to_string() {
        return Err("Order IDs do not match".into());
    }

    let mut order = order;
    order.filled_quantity = alpaca_order
        .filled_quantity
        .to_u64()
        .ok_or("Failed to convert filled quantity to u64")?;
    order.average_filled_price = alpaca_order
        .average_fill_price
        .clone()
        .map(|price| Decimal::from_str(price.to_string().as_str()))
        .transpose()
        .map_err(|e| format!("Failed to parse average fill price: {}", e))?;
    order.status = map_from_alpaca(alpaca_order.status);
    order.filled_at = map_date(alpaca_order.filled_at);
    order.expired_at = map_date(alpaca_order.expired_at);
    order.cancelled_at = map_date(alpaca_order.canceled_at);
    Ok(order)
}

pub fn map_close_order(alpaca_order: &AlpacaOrder, target: Order) -> Result<Order, Box<dyn Error>> {
    let mut order = target;
    order.broker_order_id = Some(
        Uuid::parse_str(&alpaca_order.id.to_string())
            .map_err(|e| format!("Failed to parse Alpaca order ID as UUID: {}", e))?,
    );
    order.status = map_from_alpaca(alpaca_order.status);
    order.submitted_at = map_date(alpaca_order.submitted_at);
    order.category = OrderCategory::Market;
    Ok(order)
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
        _ => OrderStatus::Unknown, // Add this line
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
            _non_exhaustive: (),
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
    fn test_map_orders_entry_id_are_different() {
        // Create a sample AlpacaOrder and Trade
        let alpaca_order = default();
        let trade = Trade::default();
        let result = map_entry(alpaca_order, &trade);
        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err().to_string(),
            "order does not have a broker id. It can not be mapped into an alpaca order"
        );
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
        let order = result.first().expect("Expected at least one order");
        assert_eq!(order.status, OrderStatus::Filled);
        assert!(order.filled_at.is_some());
        assert_eq!(order.filled_quantity, 100);
        assert_eq!(order.average_filled_price, Some(dec!(10)));
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
        let entry_order = result.first().expect("Expected entry order");
        assert_eq!(entry_order.status, OrderStatus::Filled);
        assert!(entry_order.filled_at.is_some());
        assert_eq!(entry_order.filled_quantity, 100);
        assert_eq!(entry_order.average_filled_price, Some(dec!(11)));

        // Target
        let target_order = result.get(1).expect("Expected target order");
        assert_eq!(target_order.status, OrderStatus::Filled);
        assert!(target_order.filled_at.is_some());
        assert_eq!(target_order.filled_quantity, 100);
        assert_eq!(target_order.average_filled_price, Some(dec!(10)));
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
        let entry_order = result.first().expect("Expected entry order");
        assert_eq!(entry_order.status, OrderStatus::Filled);
        assert!(entry_order.filled_at.is_some());
        assert_eq!(entry_order.filled_quantity, 100);
        assert_eq!(entry_order.average_filled_price, Some(dec!(9)));

        // Stop
        let stop_order = result.get(1).expect("Expected stop order");
        assert_eq!(stop_order.status, OrderStatus::Filled);
        assert!(stop_order.filled_at.is_some());
        assert_eq!(stop_order.filled_quantity, 100);
        assert_eq!(stop_order.average_filled_price, Some(dec!(10)));
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
            mapped_order.unwrap().broker_order_id.unwrap(),
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

        assert_eq!(mapped_order.unwrap().filled_quantity, 10);
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

        assert_eq!(
            mapped_order.unwrap().average_filled_price.unwrap(),
            dec!(2112.1212)
        );
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

        assert_eq!(mapped_order.unwrap().status, OrderStatus::Filled);
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
        assert_eq!(mapped_order.unwrap().filled_at, map_date(Some(now)));
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

        let mapped_order = map(&alpaca_order, order).unwrap();

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

        let mapped_order = map(&alpaca_order, order).unwrap();

        assert_eq!(mapped_order.cancelled_at, map_date(Some(now)));
    }
    #[test]
    fn test_map_date_with_none() {
        let expected: Option<NaiveDateTime> = None;
        assert_eq!(map_date(None), expected);
    }

    #[test]
    #[allow(clippy::cognitive_complexity)]
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
