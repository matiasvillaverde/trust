use apca::api::v2::order::{
    Amount, Class, Create, CreateReq, CreateReqInit, Order as AlpacaOrder, Side, StopLoss,
    TakeProfit, TimeInForce, Type,
};
use apca::Client;
use num_decimal::Num;

use std::str::FromStr;
use tokio::runtime::Runtime;
use uuid::Uuid;

use model::{Account, BrokerLog, Order, OrderIds, Trade, TradeCategory};
use std::error::Error;

use crate::keys;

pub fn submit_sync(
    trade: &Trade,
    account: &Account,
) -> Result<(BrokerLog, OrderIds), Box<dyn Error>> {
    assert!(trade.account_id == account.id); // Verify that the trade is for the account

    let api_info = keys::read_api_key(&account.environment, account)?;
    let client = Client::new(api_info);

    let request = new_request(trade)?;
    let order = Runtime::new()
        .map_err(|e| Box::new(e) as Box<dyn Error>)?
        .block_on(submit(client, request))?;

    let log = BrokerLog {
        trade_id: trade.id,
        log: serde_json::to_string(&order)?,
        ..Default::default()
    };
    let ids = extract_ids(&order, trade)?;
    Ok((log, ids))
}

async fn submit(
    client: Client,
    request: CreateReq,
) -> Result<apca::api::v2::order::Order, Box<dyn Error>> {
    let result = client.issue::<Create>(&request).await;

    match result {
        Ok(order) => Ok(order),
        Err(e) => {
            eprintln!("Error submitting trade: {e:?}. Are the US market open?");
            Err(Box::new(e))
        }
    }
}

fn extract_ids(order: &AlpacaOrder, trade: &Trade) -> Result<OrderIds, Box<dyn Error>> {
    let mut stop_id = None;
    let mut target_id = None;

    for leg in &order.legs {
        let leg_price = match (leg.limit_price.clone(), leg.stop_price.clone()) {
            (Some(limit_price), None) => limit_price,
            (None, Some(stop_price)) => stop_price,
            _ => return Err(format!("No price found for leg: {:?}", leg.id).into()),
        };

        if leg_price.to_string() == trade.target.unit_price.to_string() {
            target_id = Some(leg.id);
        }

        if leg_price.to_string() == trade.safety_stop.unit_price.to_string() {
            stop_id = Some(leg.id);
        }
    }

    let stop_id = stop_id.ok_or("Stop ID not found")?;
    let target_id = target_id.ok_or("Target ID not found")?;

    Ok(OrderIds {
        stop: Uuid::from_str(&stop_id.to_string())
            .map_err(|e| format!("Failed to parse stop UUID: {}", e))?,
        entry: Uuid::from_str(&order.id.to_string())
            .map_err(|e| format!("Failed to parse entry UUID: {}", e))?,
        target: Uuid::from_str(&target_id.to_string())
            .map_err(|e| format!("Failed to parse target UUID: {}", e))?,
    })
}

fn new_request(trade: &Trade) -> Result<CreateReq, Box<dyn Error>> {
    let entry = Num::from_str(trade.entry.unit_price.to_string().as_str())
        .map_err(|e| format!("Failed to parse entry price: {}", e))?;
    let stop = Num::from_str(trade.safety_stop.unit_price.to_string().as_str())
        .map_err(|e| format!("Failed to parse stop price: {}", e))?;
    let target = Num::from_str(trade.target.unit_price.to_string().as_str())
        .map_err(|e| format!("Failed to parse target price: {}", e))?;

    Ok(CreateReqInit {
        class: Class::Bracket,
        type_: Type::Limit,
        limit_price: Some(entry),
        take_profit: Some(TakeProfit::Limit(target)),
        stop_loss: Some(StopLoss::Stop(stop)),
        time_in_force: time_in_force(&trade.entry),
        extended_hours: trade.entry.extended_hours,
        client_order_id: Some(trade.entry.id.to_string()),
        ..Default::default()
    }
    .init(
        trade.trading_vehicle.symbol.to_uppercase(),
        side(trade),
        Amount::quantity(trade.entry.quantity),
    ))
}

fn time_in_force(entry: &Order) -> TimeInForce {
    match entry.time_in_force {
        model::TimeInForce::Day => TimeInForce::Day,
        model::TimeInForce::UntilCanceled => TimeInForce::UntilCanceled,
        model::TimeInForce::UntilMarketClose => TimeInForce::UntilMarketClose,
        model::TimeInForce::UntilMarketOpen => TimeInForce::UntilMarketOpen,
    }
}

pub fn side(trade: &Trade) -> Side {
    match trade.category {
        TradeCategory::Long => Side::Buy,
        TradeCategory::Short => Side::Sell,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use apca::api::v2::order::{Amount, Class, Side, Type};
    use num_decimal::Num;
    use rust_decimal_macros::dec;
    use uuid::Uuid;

    #[allow(clippy::too_many_lines)]
    fn default() -> AlpacaOrder {
        let data = r#"
        {
            "id": "b6b12dc0-8e21-4d2e-8315-907d3116a6b8",
            "client_order_id": "9fbce7ef-b98b-4930-80c1-ab929d52cfa3",
            "status": "accepted",
            "created_at": "2023-06-11T16:10:42.601331701Z",
            "updated_at": "2023-06-11T16:10:42.601331701Z",
            "submitted_at": "2023-06-11T16:10:42.600806651Z",
            "filled_at": null,
            "expired_at": null,
            "canceled_at": null,
            "asset_class": "us_equity",
            "asset_id": "386e0540-acda-4320-9290-2f453331eaf4",
            "symbol": "YPF",
            "qty": "3000",
            "filled_qty": "0",
            "type": "limit",
            "order_class": "bracket",
            "side": "buy",
            "time_in_force": "gtc",
            "limit_price": "12.55",
            "stop_price": null,
            "trail_price": null,
            "trail_percent": null,
            "filled_avg_price": null,
            "extended_hours": false,
            "legs": [
                {
                    "id": "90e41b1e-9089-444d-9f68-c204a4d32914",
                    "client_order_id": "589175f4-28e2-400a-9c5d-b001f0be8f76",
                    "status": "held",
                    "created_at": "2023-06-11T16:10:42.601392501Z",
                    "updated_at": "2023-06-11T16:10:42.601392501Z",
                    "submitted_at": "2023-06-11T16:10:42.600806651Z",
                    "filled_at": null,
                    "expired_at": null,
                    "canceled_at": null,
                    "asset_class": "us_equity",
                    "asset_id": "386e0540-acda-4320-9290-2f453331eaf4",
                    "symbol": "YPF",
                    "qty": "3000",
                    "filled_qty": "0",
                    "type": "limit",
                    "order_class": "bracket",
                    "side": "sell",
                    "time_in_force": "gtc",
                    "limit_price": "12.58",
                    "stop_price": null,
                    "trail_price": null,
                    "trail_percent": null,
                    "filled_avg_price": null,
                    "extended_hours": false,
                    "legs": []
                },
                {
                    "id": "8654f70e-3b42-4014-a9ac-5a7101989aad",
                    "client_order_id": "fffa65ea-3d2b-4cd1-a55a-faca9473060f",
                    "status": "held",
                    "created_at": "2023-06-11T16:10:42.601415221Z",
                    "updated_at": "2023-06-11T16:10:42.601415221Z",
                    "submitted_at": "2023-06-11T16:10:42.600806651Z",
                    "filled_at": null,
                    "expired_at": null,
                    "canceled_at": null,
                    "asset_class": "us_equity",
                    "asset_id": "386e0540-acda-4320-9290-2f453331eaf4",
                    "symbol": "YPF",
                    "qty": "3000",
                    "filled_qty": "0",
                    "type": "stop",
                    "order_class": "bracket",
                    "side": "sell",
                    "time_in_force": "gtc",
                    "limit_price": null,
                    "stop_price": "12.52",
                    "trail_price": null,
                    "trail_percent": null,
                    "filled_avg_price": null,
                    "extended_hours": false,
                    "legs": []
                }
            ]
        }"#;

        serde_json::from_str(data).unwrap()
    }

    #[test]
    fn test_new_request() {
        // Create a sample trade object
        let trade = Trade {
            safety_stop: Order {
                unit_price: dec!(10.27),
                ..Default::default()
            },
            entry: Order {
                unit_price: dec!(13.22),
                ..Default::default()
            },
            target: Order {
                unit_price: dec!(15.03),
                ..Default::default()
            },
            ..Default::default()
        };

        // Call the new_request function with the sample trade object
        let order_req = new_request(&trade).unwrap();

        // Check if the returned OrderReq object has the correct values
        assert_eq!(order_req.client_order_id, Some(trade.entry.id.to_string())); // The client_order_id should be the same as the entry order id.
        assert_eq!(order_req.class, Class::Bracket);
        assert_eq!(order_req.type_, Type::Limit);
        assert_eq!(
            order_req.limit_price.unwrap(),
            Num::from_str("13.22").unwrap()
        );
        assert_eq!(
            order_req.take_profit.unwrap(),
            TakeProfit::Limit(Num::from_str("15.03").unwrap())
        );
        assert_eq!(
            order_req.stop_loss.unwrap(),
            StopLoss::Stop(Num::from_str("10.27").unwrap())
        );
        assert_eq!(
            order_req.symbol.to_string(),
            trade.trading_vehicle.symbol.to_uppercase()
        );
        assert_eq!(order_req.side, side(&trade));
        assert_eq!(order_req.amount, Amount::quantity(trade.entry.quantity));
        assert_eq!(order_req.time_in_force, time_in_force(&trade.entry));
        assert_eq!(order_req.extended_hours, trade.entry.extended_hours);
    }

    #[test]
    fn test_extract_ids_stop_order() {
        // Create a sample AlpacaOrder with a Stop type
        let entry = default();
        let trade = Trade {
            safety_stop: Order {
                id: Uuid::parse_str("8654f70e-3b42-4014-a9ac-5a7101989aad").unwrap(),
                unit_price: dec!(12.52),
                ..Default::default()
            },
            entry: Order {
                id: Uuid::parse_str("b6b12dc0-8e21-4d2e-8315-907d3116a6b8").unwrap(),
                ..Default::default()
            },
            target: Order {
                id: Uuid::parse_str("90e41b1e-9089-444d-9f68-c204a4d32914").unwrap(),
                unit_price: dec!(12.58),
                ..Default::default()
            },
            ..Default::default()
        };

        // Call the extract_ids function
        let result = extract_ids(&entry, &trade).unwrap();

        // Check that the stop ID is correct and the target ID is a new UUID
        assert_eq!(
            result.stop,
            Uuid::parse_str("8654f70e-3b42-4014-a9ac-5a7101989aad").unwrap()
        );
        assert_eq!(
            result.entry,
            Uuid::parse_str("b6b12dc0-8e21-4d2e-8315-907d3116a6b8").unwrap()
        );
        assert_eq!(
            result.target,
            Uuid::parse_str("90e41b1e-9089-444d-9f68-c204a4d32914").unwrap()
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
        assert_eq!(result, Side::Buy);
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
        assert_eq!(result, Side::Sell);
    }
}
