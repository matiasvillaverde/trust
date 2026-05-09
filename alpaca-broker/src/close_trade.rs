use crate::keys;
use apca::api::v2::order::{
    Amount, Class, Create, CreateReq, CreateReqInit, Delete, Id, Order as AlpacaOrder, Side,
    TimeInForce, Type,
};
use apca::Client;
use model::{Account, BrokerLog, Order, Trade, TradeCategory};
use std::error::Error;
use tokio::runtime::Runtime;
use uuid::Uuid;

pub fn close(trade: &Trade, account: &Account) -> Result<(Order, BrokerLog), Box<dyn Error>> {
    crate::ensure_trade_account(trade, account)?;

    // Validate required input before touching keychain/network.
    let target_order_id = trade
        .target
        .broker_order_id
        .as_deref()
        .ok_or("Target order ID is missing")?;
    let target_order_id = Uuid::parse_str(target_order_id)
        .map_err(|e| format!("Target order ID is not a valid UUID: {e}"))?;

    let api_info = keys::read_api_key(&account.environment, account)?;
    let client = Client::new(api_info);

    // 1. Cancel the target order.
    Runtime::new()
        .map_err(|e| Box::new(e) as Box<dyn Error>)?
        .block_on(cancel_target(&client, target_order_id))?;

    // 2. Submit a market order to close the trade.
    let request = new_request(trade);
    let alpaca_order = Runtime::new()
        .map_err(|e| Box::new(e) as Box<dyn Error>)?
        .block_on(submit_market_order(client, request))?;

    // 3. Log the Alpaca order.
    let log = broker_log_from_order(trade, &alpaca_order)?;

    // 4. Map the Alpaca order to a Trust order.
    let order = map_close_order_from_alpaca(&alpaca_order, trade)?;

    Ok((order, log))
}

async fn cancel_target(client: &Client, order_id: Uuid) -> Result<(), Box<dyn Error>> {
    let result = client.issue::<Delete>(&Id(order_id)).await;
    match result {
        Ok(_) => Ok(()),
        Err(e) => {
            eprintln!("Error cancel target: {e:?}");
            Err(Box::new(e))
        }
    }
}

async fn submit_market_order(
    client: Client,
    request: CreateReq,
) -> Result<AlpacaOrder, Box<dyn Error>> {
    let result = client.issue::<Create>(&request).await;

    match result {
        Ok(order) => Ok(order),
        Err(e) => {
            eprintln!("Error posting cancel trade: {e:?}");
            Err(Box::new(e))
        }
    }
}

fn broker_log_from_order(trade: &Trade, order: &AlpacaOrder) -> Result<BrokerLog, Box<dyn Error>> {
    Ok(BrokerLog {
        trade_id: trade.id,
        log: serde_json::to_string(order)?,
        ..Default::default()
    })
}

fn map_close_order_from_alpaca(
    alpaca_order: &AlpacaOrder,
    trade: &Trade,
) -> Result<Order, Box<dyn Error>> {
    crate::order_mapper::map_close_order(alpaca_order, trade.target.clone())
}

fn new_request(trade: &Trade) -> CreateReq {
    CreateReqInit {
        class: Class::Simple,
        type_: Type::Market,
        time_in_force: TimeInForce::UntilCanceled,
        extended_hours: trade.target.extended_hours,
        ..Default::default()
    }
    .init(
        trade.trading_vehicle.symbol.to_uppercase(),
        side(trade),
        Amount::quantity(trade.entry.quantity),
    )
}

pub fn side(trade: &Trade) -> Side {
    match trade.category {
        TradeCategory::Long => Side::Sell,
        TradeCategory::Short => Side::Buy,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use apca::api::v2::order::{Amount, Class, Side, Type};
    use chrono::{DateTime, Utc};
    use model::{Account, OrderCategory, OrderStatus, Trade};

    fn utc(value: &str) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339(value)
            .expect("valid timestamp")
            .with_timezone(&Utc)
    }

    fn alpaca_order() -> AlpacaOrder {
        let raw = r#"{
            "id": "f90e12e8-5c4d-4f9b-8c4d-060d99539d08",
            "client_order_id": "close-client-order",
            "status": "accepted",
            "created_at": "2026-05-07T13:00:00Z",
            "updated_at": null,
            "submitted_at": "2026-05-07T13:00:01Z",
            "filled_at": null,
            "expired_at": null,
            "canceled_at": null,
            "asset_class": "us_equity",
            "asset_id": "386e0540-acda-4320-9290-2f453331eaf4",
            "symbol": "AAPL",
            "qty": "10",
            "filled_qty": "0",
            "type": "market",
            "order_class": "simple",
            "side": "sell",
            "time_in_force": "gtc",
            "limit_price": null,
            "stop_price": null,
            "trail_price": null,
            "trail_percent": null,
            "filled_avg_price": null,
            "extended_hours": false,
            "legs": []
        }"#;

        serde_json::from_str(raw).expect("valid alpaca order")
    }

    #[test]
    fn test_new_request() {
        // Create a sample trade object
        let trade = Trade::default();

        // Call the new_request function with the sample trade object
        let order_req = new_request(&trade);

        // Check if the returned OrderReq object has the correct values
        assert_eq!(order_req.class, Class::Simple);
        assert_eq!(order_req.type_, Type::Market);
        assert_eq!(
            order_req.symbol.to_string(),
            trade.trading_vehicle.symbol.to_uppercase()
        );
        assert_eq!(order_req.side, Side::Sell);
        assert_eq!(order_req.amount, Amount::quantity(trade.entry.quantity));
        assert_eq!(order_req.time_in_force, TimeInForce::UntilCanceled);
        assert_eq!(order_req.extended_hours, trade.target.extended_hours);
    }

    #[test]
    fn new_request_uses_target_extended_hours_flag() {
        let trade = Trade {
            target: model::Order {
                extended_hours: true,
                ..Default::default()
            },
            entry: model::Order {
                extended_hours: false,
                ..Default::default()
            },
            ..Default::default()
        };

        let order_req = new_request(&trade);

        assert!(order_req.extended_hours);
    }

    #[test]
    fn broker_log_from_order_serializes_close_order_for_trade() {
        let trade = Trade::default();
        let order = alpaca_order();

        let log = broker_log_from_order(&trade, &order).expect("broker log should serialize");
        let value: serde_json::Value =
            serde_json::from_str(&log.log).expect("serialized order is json");

        assert_eq!(log.trade_id, trade.id);
        assert_eq!(
            value.get("id").and_then(serde_json::Value::as_str),
            Some("f90e12e8-5c4d-4f9b-8c4d-060d99539d08")
        );
    }

    #[test]
    fn map_close_order_from_alpaca_replaces_target_broker_fields() {
        let trade = Trade {
            target: model::Order {
                broker_order_id: Some("old-target-id".to_string()),
                ..Default::default()
            },
            ..Default::default()
        };
        let order = alpaca_order();

        let mapped =
            map_close_order_from_alpaca(&order, &trade).expect("close order should map cleanly");

        assert_eq!(
            mapped.broker_order_id.as_deref(),
            Some("f90e12e8-5c4d-4f9b-8c4d-060d99539d08")
        );
        assert_eq!(mapped.status, OrderStatus::Accepted);
        assert_eq!(mapped.category, OrderCategory::Market);
        assert_eq!(
            mapped.submitted_at,
            Some(utc("2026-05-07T13:00:01Z").naive_utc())
        );
    }

    #[test]
    fn test_side_long_trade() {
        // Create a sample Trade with Long category
        let trade = Trade {
            category: TradeCategory::Long,
            ..Default::default()
        };

        // Call the side function
        let result = side(&trade);

        // Check that the result is Side::Buy
        assert_eq!(result, Side::Sell);
    }

    #[test]
    fn test_side_short_trade() {
        // Create a sample Trade with Short category
        let trade = Trade {
            category: TradeCategory::Short,
            ..Default::default()
        };

        // Call the side function
        let result = side(&trade);

        // Check that the result is Side::Sell
        assert_eq!(result, Side::Buy);
    }

    #[test]
    fn close_returns_error_when_target_broker_order_id_is_missing() {
        let account = Account::default();
        let trade = Trade {
            account_id: account.id,
            ..Trade::default()
        };

        let err = close(&trade, &account).expect_err("missing target order id should fail");
        assert!(err.to_string().contains("Target order ID is missing"));
    }

    #[test]
    fn close_returns_error_when_target_broker_order_id_is_invalid() {
        let account = Account::default();
        let trade = Trade {
            account_id: account.id,
            target: model::Order {
                broker_order_id: Some("not-a-uuid".to_string()),
                ..Default::default()
            },
            ..Trade::default()
        };

        let err = close(&trade, &account).expect_err("invalid target order id should fail");
        assert!(err
            .to_string()
            .contains("Target order ID is not a valid UUID"));
    }

    #[test]
    fn close_returns_error_when_trade_account_mismatch() {
        let account = Account::default();
        let trade = Trade {
            account_id: uuid::Uuid::new_v4(),
            ..Trade::default()
        };

        let err = close(&trade, &account).expect_err("account mismatch should fail");
        assert!(err
            .to_string()
            .contains("Trade account does not match broker account"));
    }
}
