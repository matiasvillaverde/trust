use crate::order_mapper;
use apca::api::v2::order::Order as AlpacaOrder;
use apca::api::v2::orders::{Get, OrdersReq, Status as AlpacaRequestStatus};
use apca::Client;
use std::error::Error;
use trust_model::{Order, Status, Trade};
use uuid::Uuid;

pub async fn sync_trade(
    client: &Client,
    trade: &Trade,
) -> Result<(Status, Vec<Order>), Box<dyn Error>> {
    // 1. Get closed orders from Alpaca
    let orders = get_closed_orders(client, trade).await?;

    // 2. Find entry order
    let entry_order = find_entry(orders, trade.id)?;

    // 3. Map entry order that has Stop and Target as legs.
    let updated_orders = order_mapper::map_orders(entry_order, trade)?;

    // 4. Update Trade Status
    let status = order_mapper::map_trade_status(trade, &updated_orders);

    // 5. Return updated orders and status
    Ok((status, updated_orders))
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

fn find_entry(orders: Vec<AlpacaOrder>, trade_id: Uuid) -> Result<AlpacaOrder, Box<dyn Error>> {
    let entry_order = orders
        .into_iter()
        .find(|x| x.client_order_id == trade_id.to_string());

    match entry_order {
        Some(order) => Ok(order),
        None => return Err("Entry order not found".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use apca::api::v2::order::{Amount, Class, Side, TimeInForce, Type};
    use apca::api::v2::order::{Order as AlpacaOrder, Status as AlpacaStatus};
    use apca::api::v2::{asset, order::Id};
    use chrono::Utc;
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
            asset_id: asset::Id(Uuid::new_v4()),
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
    fn test_find_entry() {
        let id = Uuid::new_v4();
        let mut entry_order = default();
        entry_order.client_order_id = id.to_string();

        // Create some sample orders
        let orders = vec![
            default(),
            default(),
            default(),
            default(),
            default(),
            default(),
            entry_order.clone(),
            default(),
            default(),
            default(),
            default(),
        ];

        let result_1 = find_entry(orders, id);
        assert_eq!(result_1.unwrap(), entry_order);
    }

    #[test]
    fn test_find_entry_does_not_exist() {
        // Create some sample orders
        let orders = vec![
            default(),
            default(),
            default(),
            default(),
            default(),
            default(),
            default(),
            default(),
            default(),
            default(),
        ];

        let result_1 = find_entry(orders, Uuid::new_v4()).expect_err("Should not find entry order");
        assert_eq!(result_1.to_string(), "Entry order not found");
    }
}
