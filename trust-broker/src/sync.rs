use crate::order_mapper;
use apca::api::v2::order::Order as AlpacaOrder;
use apca::api::v2::orders::{Get, OrdersReq, Status as AlpacaRequestStatus};
use apca::Client;
use std::error::Error;
use trust_model::{Order, Status, Trade};

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
