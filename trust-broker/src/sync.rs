use crate::order_mapper;
use apca::api::v2::order::Order as AlpacaOrder;
use apca::api::v2::orders::{Get, OrdersReq, Status as AlpacaRequestStatus};
use apca::Client;
use std::error::Error;
use trust_model::{Order, OrderStatus, Status, Trade};

pub async fn sync_trade(
    client: &Client,
    trade: &Trade,
) -> Result<(Status, Vec<Order>), Box<dyn Error>> {
    // 1. Get closed orders from Alpaca
    let orders = get_closed_orders(client, trade).await?;

    // 2. Find entry order
    let entry_order = orders
        .into_iter()
        .find(|x| x.client_order_id == trade.id.to_string());

    let entry_order = match entry_order {
        Some(order) => order,
        None => return Err("Entry order not found".into()),
    };

    // 3. Map entry order that has Stop and Target as legs.
    map_orders(entry_order, trade)
}

/// Get closed orders from Alpaca
async fn get_closed_orders(
    client: &Client,
    trade: &Trade,
) -> Result<Vec<AlpacaOrder>, Box<dyn Error>> {
    let request: OrdersReq = OrdersReq {
        symbols: vec![trade.trading_vehicle.symbol.to_string()],
        status: AlpacaRequestStatus::Closed,
        ..Default::default()
    };

    let orders = client.issue::<Get>(&request).await.unwrap();

    Ok(orders)
}

// TODO: Test this and refactor it
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
            // 1. Map target order to our domain model.
            let order = order_mapper::map(&order, trade.target.clone());

            // 2. If the target is filled, then the trade status is ClosedTarget.
            if order.status == OrderStatus::Filled {
                trade_status = Status::ClosedTarget;
            }

            // 3. If the target is updated, then we add it to the updated orders.
            if order != trade.target {
                updated_orders.push(order);
            }
        } else if order.id.to_string() == trade.safety_stop.broker_order_id.unwrap().to_string() {
            // 1. Map stop order to our domain model.
            let order = order_mapper::map(&order, trade.safety_stop.clone());

            // 2. If the stop is filled, then the trade status is ClosedStopLoss.
            if order.status == OrderStatus::Filled {
                trade_status = Status::ClosedStopLoss;
            }

            // 3. If the stop is updated, then we add it to the updated orders.
            if order != trade.safety_stop {
                updated_orders.push(order);
            }
        }
    }

    // 4. Map entry order to our domain model.
    let entry_order = order_mapper::map(&entry_order, trade.entry.clone());

    // 5. If the entry is filled and the target and stop are not, then the trade status is filled.
    if trade_status == Status::Submitted && entry_order.status == OrderStatus::Filled {
        trade_status = Status::Filled;
    }

    // 6. If the entry is updated, then we add it to the updated orders.
    if entry_order != trade.entry {
        updated_orders.push(entry_order);
    }

    Ok((trade_status, updated_orders))
}

#[cfg(test)]
mod tests {
    use super::*;
    use apca::api::v2::order::{Amount, Class, Side, Status as AlpacaStatus, TimeInForce, Type};
    use apca::api::v2::{asset, order::Id};
    use chrono::Utc;
    use num_decimal::Num;
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
}
