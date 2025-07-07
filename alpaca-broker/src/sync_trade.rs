use crate::keys;
use crate::order_mapper;
use apca::api::v2::order::Order as AlpacaOrder;
use apca::api::v2::orders::{List, ListReq, Status as AlpacaRequestStatus};
use apca::Client;
use model::{Account, BrokerLog, Order, Status, Trade};
use std::error::Error;
use tokio::runtime::Runtime;

pub fn sync(
    trade: &Trade,
    account: &Account,
) -> Result<(Status, Vec<Order>, BrokerLog), Box<dyn Error>> {
    assert!(trade.account_id == account.id); // Verify that the trade is for the account

    let api_info = keys::read_api_key(&account.environment, account)?;
    let client = Client::new(api_info);

    let orders = Runtime::new()
        .map_err(|e| Box::new(e) as Box<dyn Error>)?
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
    let updated_orders = match trade.status {
        Status::Canceled => {
            find_target(orders, trade).and_then(|order| order_mapper::map_target(order, trade))
        }
        _ => find_entry(orders, trade).and_then(|order| order_mapper::map_entry(order, trade)),
    }?;

    let status = order_mapper::map_trade_status(trade, &updated_orders);

    Ok((status, updated_orders))
}

/// Get closed orders from Alpaca
async fn get_closed_orders(
    client: &Client,
    trade: &Trade,
) -> Result<Vec<AlpacaOrder>, Box<dyn Error>> {
    let request: ListReq = ListReq {
        symbols: vec![trade.trading_vehicle.symbol.to_string()],
        status: AlpacaRequestStatus::Closed,
        ..Default::default()
    };

    let orders = client
        .issue::<List>(&request)
        .await
        .map_err(|e| Box::new(e) as Box<dyn Error>)?;
    Ok(orders)
}

/// Find entry order from closed orders
pub fn find_entry(orders: Vec<AlpacaOrder>, trade: &Trade) -> Result<AlpacaOrder, Box<dyn Error>> {
    orders
        .into_iter()
        .find(|x| x.client_order_id == trade.entry.id.to_string())
        .ok_or_else(|| "Entry order not found, it can be that is not filled yet".into())
}

/// Find the target order that is on the first level of the JSON
pub fn find_target(orders: Vec<AlpacaOrder>, trade: &Trade) -> Result<AlpacaOrder, Box<dyn Error>> {
    let target_order_id = trade
        .target
        .broker_order_id
        .ok_or("Target order ID is missing")?;

    orders
        .into_iter()
        .find(|x| x.id.to_string() == target_order_id.to_string())
        .ok_or_else(|| "Target order not found, it can be that is not filled yet".into())
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
            _non_exhaustive: (),
        }
    }

    fn manually_closed_target() -> Vec<AlpacaOrder> {
        let data = r#"
        [
    {
        "id": "6a3a0ab0-8846-4369-b9f5-2351a316ae0f",
        "client_order_id": "a4e2da32-ed89-43e8-827f-db373db07449",
        "status": "filled",
        "created_at": "2023-06-20T14:30:38.644640192Z",
        "updated_at": "2023-06-20T14:30:39.201916476Z",
        "submitted_at": "2023-06-20T14:30:38.651964022Z",
        "filled_at": "2023-06-20T14:30:39.198984174Z",
        "expired_at": null,
        "canceled_at": null,
        "asset_class": "us_equity",
        "asset_id": "8ccae427-5dd0-45b3-b5fe-7ba5e422c766",
        "symbol": "TSLA",
        "qty": "100",
        "filled_qty": "100",
        "type": "market",
        "order_class": "simple",
        "side": "sell",
        "time_in_force": "gtc",
        "limit_price": null,
        "stop_price": null,
        "trail_price": null,
        "trail_percent": null,
        "filled_avg_price": "262.12",
        "extended_hours": false,
        "legs": []
    },
    {
        "id": "54c8a893-0473-425f-84de-6f9c48197ed6",
        "client_order_id": "3379dcc6-f979-42f3-a3d5-6465519f2c8e",
        "status": "filled",
        "created_at": "2023-06-20T14:22:16.555854427Z",
        "updated_at": "2023-06-20T14:22:16.873225184Z",
        "submitted_at": "2023-06-20T14:22:16.564270239Z",
        "filled_at": "2023-06-20T14:22:16.869638508Z",
        "expired_at": null,
        "canceled_at": null,
        "asset_class": "us_equity",
        "asset_id": "8ccae427-5dd0-45b3-b5fe-7ba5e422c766",
        "symbol": "TSLA",
        "qty": "100",
        "filled_qty": "100",
        "type": "limit",
        "order_class": "bracket",
        "side": "buy",
        "time_in_force": "gtc",
        "limit_price": "264",
        "stop_price": null,
        "trail_price": null,
        "trail_percent": null,
        "filled_avg_price": "263.25",
        "extended_hours": false,
        "legs": [
            {
                "id": "823b5272-ee9b-4783-bc45-c769f5cb24d1",
                "client_order_id": "7c2e396a-b111-4d6d-b283-2f13c44b94bc",
                "status": "canceled",
                "created_at": "2023-06-20T14:22:16.555889537Z",
                "updated_at": "2023-06-20T14:30:37.762708578Z",
                "submitted_at": "2023-06-20T14:22:16.890032267Z",
                "filled_at": null,
                "expired_at": null,
                "canceled_at": "2023-06-20T14:30:37.759320757Z",
                "asset_class": "us_equity",
                "asset_id": "8ccae427-5dd0-45b3-b5fe-7ba5e422c766",
                "symbol": "TSLA",
                "qty": "100",
                "filled_qty": "0",
                "type": "limit",
                "order_class": "bracket",
                "side": "sell",
                "time_in_force": "gtc",
                "limit_price": "280",
                "stop_price": null,
                "trail_price": null,
                "trail_percent": null,
                "filled_avg_price": null,
                "extended_hours": false,
                "legs": []
            },
            {
                "id": "dd4fdc18-f82b-40c4-9cee-9c1522e62e74",
                "client_order_id": "6f0ce7ef-9b4b-425a-9278-e9516945b58c",
                "status": "canceled",
                "created_at": "2023-06-20T14:22:16.555915187Z",
                "updated_at": "2023-06-20T14:30:37.753179958Z",
                "submitted_at": "2023-06-20T14:22:16.555095977Z",
                "filled_at": null,
                "expired_at": null,
                "canceled_at": "2023-06-20T14:30:37.753179268Z",
                "asset_class": "us_equity",
                "asset_id": "8ccae427-5dd0-45b3-b5fe-7ba5e422c766",
                "symbol": "TSLA",
                "qty": "100",
                "filled_qty": "0",
                "type": "stop",
                "order_class": "bracket",
                "side": "sell",
                "time_in_force": "gtc",
                "limit_price": null,
                "stop_price": "260",
                "trail_price": null,
                "trail_percent": null,
                "filled_avg_price": null,
                "extended_hours": false,
                "legs": []
            }
        ]}
            ]
        "#;
        serde_json::from_str(data).unwrap()
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
            status: Status::Filled,
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
    fn test_sync_trade_manually_closed() {
        let target_id = Uuid::parse_str("6a3a0ab0-8846-4369-b9f5-2351a316ae0f").unwrap();

        // 1. Create a Target that is a child order of the Entry
        let target_order = Order {
            broker_order_id: Some(target_id),
            unit_price: dec!(247),
            ..Default::default()
        };

        let trade = Trade {
            target: target_order,
            status: Status::Canceled,
            ..Default::default()
        };

        // Json data with manually closed target from Alpaca
        let orders = manually_closed_target();

        let (status, updated_orders) = sync_trade(&trade, orders).unwrap();

        // Assert that the orders has been updated
        assert_eq!(status, Status::ClosedTarget);
        assert_eq!(updated_orders.len(), 1);
        assert_eq!(updated_orders[0].broker_order_id, Some(target_id));
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

        let orders = vec![default(); 5];
        let mut all_orders = vec![entry_order.clone()];
        all_orders.extend(orders);
        all_orders.resize(12, default());

        let result_1 = find_entry(all_orders, &trade);
        assert_eq!(result_1.unwrap(), entry_order);
    }

    #[test]
    fn test_find_target() {
        let id = Uuid::parse_str("6a3a0ab0-8846-4369-b9f5-2351a316ae0f").unwrap();

        let trade = Trade {
            target: Order {
                broker_order_id: Some(id),
                ..Default::default()
            },
            ..Default::default()
        };

        // Sample orders from JSON coming from Alpaca
        let orders = manually_closed_target();

        let result = find_target(orders, &trade);

        // Assert that it find the order with the same target id
        assert_eq!(result.unwrap().id.to_string(), id.to_string());
    }

    #[test]
    fn test_find_entry_does_not_exist() {
        // Create a sample order
        let orders = vec![default(); 5];

        assert!(
            find_entry(orders, &Trade::default()).is_err(),
            "Should not find entry order"
        );
    }
}
