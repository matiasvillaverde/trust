use apca::api::v2::order::{Order as AlpacaOrder, Status as AlpacaStatus};
use chrono::DateTime;
use chrono::NaiveDateTime;
use chrono::Utc;
use rust_decimal::Decimal;
use trust_model::{Order, OrderStatus};

pub fn map(alpaca_order: &AlpacaOrder, order: Order) -> Order {
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
        alpaca_order.average_fill_price = Some(Num::from(2112));

        let order = Order {
            broker_order_id: Some(Uuid::parse_str("00000000-0000-0000-0000-000000000000").unwrap()),
            ..Default::default()
        };

        let mapped_order = map(&alpaca_order, order);

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
