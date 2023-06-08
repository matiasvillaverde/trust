use apca::api::v2::order::Status;
use apca::api::v2::orders::{Get, OrdersReq};
use apca::Client;

use trust_model::{Order, TradingVehicle};

pub async fn fill_order(client: Client, order: &Order, tv: &TradingVehicle) {
    assert!(order.trading_vehicle_id == tv.id); // Verify that the order is for the trading vehicle

    let request = OrdersReq {
        symbols: vec![tv.symbol.to_string()],
        ..Default::default()
    };

    let orders = client.issue::<Get>(&request).await.unwrap();
    let order = orders
        .into_iter()
        .find(|x| x.id.to_string() == order.broker_order_id.unwrap().to_string());

    let order = order.unwrap();

    println!("Order: {:?}", order);

    match order.status {
        Status::PartiallyFilled => println!("Order partially filled"),
        Status::Filled => println!("Order filled"),
        _ => println!("Order not filled"),
    }
}
