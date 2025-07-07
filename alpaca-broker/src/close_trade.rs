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
    assert!(trade.account_id == account.id); // Verify that the trade is for the account

    let api_info = keys::read_api_key(&account.environment, account)?;
    let client = Client::new(api_info);

    // 1. Cancel the target order.
    let target_order_id = trade
        .target
        .broker_order_id
        .ok_or("Target order ID is missing")?;

    Runtime::new()
        .map_err(|e| Box::new(e) as Box<dyn Error>)?
        .block_on(cancel_target(&client, target_order_id))?;

    // 2. Submit a market order to close the trade.
    let request = new_request(trade);
    let alpaca_order = Runtime::new()
        .map_err(|e| Box::new(e) as Box<dyn Error>)?
        .block_on(submit_market_order(client, request))?;

    // 3. Log the Alpaca order.
    let log = BrokerLog {
        trade_id: trade.id,
        log: serde_json::to_string(&alpaca_order)?,
        ..Default::default()
    };

    // 4. Map the Alpaca order to a Trust order.
    let order: Order = crate::order_mapper::map_close_order(&alpaca_order, trade.target.clone())?;

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
        assert_eq!(order_req.extended_hours, trade.entry.extended_hours);
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
}
