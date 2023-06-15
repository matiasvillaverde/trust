use crate::keys;
use crate::order_mapper;
use apca::api::v2::order::Order as AlpacaOrder;
use apca::api::v2::orders::{Get, OrdersReq, Status as AlpacaRequestStatus};
use apca::Client;
use std::error::Error;
use tokio::runtime::Runtime;
use trust_model::{Account, BrokerLog, Order, Status, Trade};

pub fn sync(
    trade: &Trade,
    account: &Account,
) -> Result<(Status, Vec<Order>, BrokerLog), Box<dyn Error>> {
    assert!(trade.account_id == account.id); // Verify that the trade is for the account

    let api_info = keys::read_api_key(&account.environment, account)?;
    let client = Client::new(api_info);

    let orders = Runtime::new()
        .unwrap()
        .block_on(get_closed_orders(&client, trade))?;

    let log = BrokerLog {
        trade_id: trade.id,
        log: serde_json::to_string(&orders)?,
        ..Default::default()
    };

    let (status, updated_orders) = sync_trade(trade, orders)?;
    Ok((status, updated_orders, log))
}

/// Sync Trade with Alpaca and return updated orders and status
fn sync_trade(
    trade: &Trade,
    orders: Vec<AlpacaOrder>,
) -> Result<(Status, Vec<Order>), Box<dyn Error>> {
    // 1. Find entry order
    let entry_order = find_entry(orders, trade)?;

    // 2. Map entry order that has Stop and Target as legs.
    let updated_orders = order_mapper::map_orders(entry_order, trade)?;

    // 3. Update Trade Status
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

    //println!("Orders: {:?}", serde_json::to_value(orders)?);

    Ok(orders)
}

/// Find entry order from closed orders
fn find_entry(orders: Vec<AlpacaOrder>, trade: &Trade) -> Result<AlpacaOrder, Box<dyn Error>> {
    orders
        .into_iter()
        .find(|x| x.client_order_id == trade.entry.id.to_string())
        .ok_or_else(|| "Entry order not found, it can be that is not filled yet".into())
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

    fn default_from_json() -> Vec<AlpacaOrder> {
        let data = r#"
        [
    {
        "id": "66b4dfbf-2905-4a25-a388-873fec1a15de",
        "client_order_id": "8ff773c7-f7ac-4220-9824-613d5921fbad",
        "status": "filled",
        "created_at": "2023-06-12T16: 22: 06.980875700Z",
        "updated_at": "2023-06-12T16: 22: 49.063255005Z",
        "submitted_at": "2023-06-12T16: 22: 06.986565167Z",
        "filled_at": "2023-06-12T16: 22: 49.060636784Z",
        "expired_at": null,
        "canceled_at": null,
        "asset_class": "us_equity",
        "asset_id": "8ccae427-5dd0-45b3-b5fe-7ba5e422c766",
        "symbol": "TSLA",
        "qty": "10",
        "filled_qty": "10",
        "type": "limit",
        "order_class": "bracket",
        "side": "buy",
        "time_in_force": "gtc",
        "limit_price": "246.2",
        "stop_price": null,
        "trail_price": null,
        "trail_percent": null,
        "filled_avg_price": "246.15",
        "extended_hours": false,
        "legs": [
            {
                "id": "99106145-92dc-477e-b1c5-fcfdee452633",
                "client_order_id": "8221d144-1bb7-4bcc-ad34-dd6b8f2c731b",
                "status": "filled",
                "created_at": "2023-06-12T16: 22: 06.980936160Z",
                "updated_at": "2023-06-12T16: 28: 09.033362252Z",
                "submitted_at": "2023-06-12T16: 22: 49.078537163Z",
                "filled_at": "2023-06-12T16: 28: 09.031428954Z",
                "expired_at": null,
                "canceled_at": null,
                "asset_class": "us_equity",
                "asset_id": "8ccae427-5dd0-45b3-b5fe-7ba5e422c766",
                "symbol": "TSLA",
                "qty": "10",
                "filled_qty": "10",
                "type": "limit",
                "order_class": "bracket",
                "side": "sell",
                "time_in_force": "gtc",
                "limit_price": "247",
                "stop_price": null,
                "trail_price": null,
                "trail_percent": null,
                "filled_avg_price": "247.009",
                "extended_hours": false,
                "legs": []
            },
            {
                "id": "ef022523-1f49-49e6-a1c1-98e2efd2ff35",
                "client_order_id": "7ce907f1-ac5e-4ec0-a566-9ae30195255f",
                "status": "canceled",
                "created_at": "2023-06-12T16: 22: 06.980963190Z",
                "updated_at": "2023-06-12T16: 28: 09.033528902Z",
                "submitted_at": "2023-06-12T16: 22: 06.980510170Z",
                "filled_at": null,
                "expired_at": null,
                "canceled_at": "2023-06-12T16: 28: 09.033526872Z",
                "asset_class": "us_equity",
                "asset_id": "8ccae427-5dd0-45b3-b5fe-7ba5e422c766",
                "symbol": "TSLA",
                "qty": "10",
                "filled_qty": "0",
                "type": "stop",
                "order_class": "bracket",
                "side": "sell",
                "time_in_force": "gtc",
                "limit_price": null,
                "stop_price": "240",
                "trail_price": null,
                "trail_percent": null,
                "filled_avg_price": null,
                "extended_hours": false,
                "legs": []
            }
        ]
    },
    {
        "id": "143ce271-912b-4fec-9051-8ab02fe2348e",
        "client_order_id": "8371f9c9-6a23-4605-9a8c-f13c825f88a9",
        "status": "filled",
        "created_at": "2023-06-12T13: 54: 05.863801183Z",
        "updated_at": "2023-06-12T13: 54: 39.483215466Z",
        "submitted_at": "2023-06-12T13: 54: 05.872465830Z",
        "filled_at": "2023-06-12T13: 54: 39.481038892Z",
        "expired_at": null,
        "canceled_at": null,
        "asset_class": "us_equity",
        "asset_id": "8ccae427-5dd0-45b3-b5fe-7ba5e422c766",
        "symbol": "TSLA",
        "qty": "1",
        "filled_qty": "1",
        "type": "limit",
        "order_class": "bracket",
        "side": "buy",
        "time_in_force": "gtc",
        "limit_price": "247",
        "stop_price": null,
        "trail_price": null,
        "trail_percent": null,
        "filled_avg_price": "246.98",
        "extended_hours": false,
        "legs": [
            {
                "id": "c697f154-ca9e-4412-8641-5afb807639ee",
                "client_order_id": "4e6c7da0-9504-480b-833b-368802bfc4da",
                "status": "filled",
                "created_at": "2023-06-12T13: 54: 05.863841103Z",
                "updated_at": "2023-06-12T13: 56: 52.249336756Z",
                "submitted_at": "2023-06-12T13: 54: 39.502858870Z",
                "filled_at": "2023-06-12T13: 56: 52.246764992Z",
                "expired_at": null,
                "canceled_at": null,
                "asset_class": "us_equity",
                "asset_id": "8ccae427-5dd0-45b3-b5fe-7ba5e422c766",
                "symbol": "TSLA",
                "qty": "1",
                "filled_qty": "1",
                "type": "limit",
                "order_class": "bracket",
                "side": "sell",
                "time_in_force": "gtc",
                "limit_price": "248",
                "stop_price": null,
                "trail_price": null,
                "trail_percent": null,
                "filled_avg_price": "248.02",
                "extended_hours": false,
                "legs": []
            },
            {
                "id": "6a0fff66-ce12-4ec9-8805-f4baf49869e2",
                "client_order_id": "288a3bd9-3027-4707-89d1-021e2b96fb96",
                "status": "canceled",
                "created_at": "2023-06-12T13: 54: 05.863865753Z",
                "updated_at": "2023-06-12T13: 56: 52.249475365Z",
                "submitted_at": "2023-06-12T13: 54: 05.863381453Z",
                "filled_at": null,
                "expired_at": null,
                "canceled_at": "2023-06-12T13: 56: 52.249474685Z",
                "asset_class": "us_equity",
                "asset_id": "8ccae427-5dd0-45b3-b5fe-7ba5e422c766",
                "symbol": "TSLA",
                "qty": "1",
                "filled_qty": "0",
                "type": "stop",
                "order_class": "bracket",
                "side": "sell",
                "time_in_force": "gtc",
                "limit_price": null,
                "stop_price": "246",
                "trail_price": null,
                "trail_percent": null,
                "filled_avg_price": null,
                "extended_hours": false,
                "legs": []
            }
        ]
    }
]"#;

        serde_json::from_str(data).unwrap()
    }

    #[test]
    fn test_sync_trade() {
        let entry_id = Uuid::parse_str("8ff773c7-f7ac-4220-9824-613d5921fbad").unwrap();
        let entry_broker_id = Uuid::parse_str("66b4dfbf-2905-4a25-a388-873fec1a15de").unwrap();
        let target_id = Uuid::parse_str("99106145-92dc-477e-b1c5-fcfdee452633").unwrap();
        let stop_id = Uuid::parse_str("ef022523-1f49-49e6-a1c1-98e2efd2ff35").unwrap();

        // 1. Create an Entry that is the parent order.
        let entry_order = Order {
            id: entry_id,
            broker_order_id: Some(entry_broker_id),
            unit_price: dec!(246.2),
            ..Default::default()
        };

        // 2. Create a Target that is a child order of the Entry
        let target_order = Order {
            broker_order_id: Some(target_id),
            unit_price: dec!(247),
            ..Default::default()
        };

        // 3. Create a Stop that is a child order of the Entry
        let stop_order = Order {
            broker_order_id: Some(stop_id),
            unit_price: dec!(240),
            ..Default::default()
        };

        let trade = Trade {
            entry: entry_order,
            target: target_order,
            safety_stop: stop_order,
            ..Default::default()
        };

        // Create some sample orders
        let orders = default_from_json();

        let (status, updated_orders) = sync_trade(&trade, orders).unwrap();

        // Assert that the orders has been updated
        assert_eq!(status, Status::ClosedTarget);
        assert_eq!(updated_orders.len(), 3);
    }

    #[test]
    fn test_find_entry() {
        let id = Uuid::new_v4();
        let mut entry_order = default();
        entry_order.client_order_id = id.to_string();

        let trade = Trade {
            entry: Order {
                id,
                ..Default::default()
            },
            ..Default::default()
        };

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

        let result_1 = find_entry(orders, &trade);
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

        let result_1 =
            find_entry(orders, &Trade::default()).expect_err("Should not find entry order");
        assert_eq!(
            result_1.to_string(),
            "Entry order not found, it can be that is not filled yet"
        );
    }
}
