use apca::api::v2::order::{
    Amount, Class, OrderReq, OrderReqInit, Post, Side, StopLoss, TakeProfit, TimeInForce, Type,
};
use apca::ApiInfo;
use apca::Client;

use num_decimal::Num;
use rust_decimal::prelude::ToPrimitive;
use tokio::runtime::Runtime;
use uuid::Uuid;

use std::error::Error;
use trust_model::{Broker, BrokerLog, Order, Trade, TradeCategory};

pub struct AlpacaBroker;

impl AlpacaBroker {
    pub fn new() -> Self {
        AlpacaBroker {}
    }
}

impl Broker for AlpacaBroker {
    fn submit_trade(&self, trade: &Trade) -> Result<BrokerLog, Box<dyn Error>> {
        let api_info = read_api_key();
        let client = Client::new(api_info);

        let request = new_request(trade);
        let order = Runtime::new().unwrap().block_on(submit(client, request))?;

        Ok(new_log(trade, format!("{:?}", order)))
    }
}

fn read_api_key() -> ApiInfo {
    let url = dotenv::var("ALPACA_API_URL").expect("ALPACA_API_URL must be set");
    let key_id = dotenv::var("ALPACA_API_KEY_ID").expect("ALPACA_API_KEY_ID must be set");
    let secret = dotenv::var("ALPACA_API_SECRET").expect("ALPACA_API_SECRET must be set");
    ApiInfo::from_parts(url, key_id, secret).unwrap()
}

async fn submit(
    client: Client,
    request: OrderReq,
) -> Result<apca::api::v2::order::Order, Box<dyn Error>> {
    let result = client.issue::<Post>(&request).await;

    match result {
        Ok(order) => Ok(order),
        Err(e) => Err(Box::new(e)),
    }
}

fn new_log(trade: &Trade, log: String) -> BrokerLog {
    let now = chrono::Utc::now().naive_utc();
    BrokerLog {
        id: Uuid::new_v4(),
        created_at: now,
        updated_at: now,
        deleted_at: None,
        trade_id: trade.id,
        log,
    }
}

fn new_request(trade: &Trade) -> OrderReq {
    let entry = Num::from(trade.entry.unit_price.amount.to_u128().unwrap());
    let stop = Num::from(trade.safety_stop.unit_price.amount.to_u128().unwrap());
    let target = Num::from(trade.target.unit_price.amount.to_u128().unwrap());

    OrderReqInit {
        class: Class::Bracket,
        type_: Type::Limit,
        limit_price: Some(entry),
        take_profit: Some(TakeProfit::Limit(target)),
        stop_loss: Some(StopLoss::Stop(stop)),
        time_in_force: time_in_force(&trade.entry),
        extended_hours: trade.entry.extended_hours,
        ..Default::default()
    }
    .init(
        trade.trading_vehicle.symbol.to_uppercase(),
        side(trade),
        Amount::quantity(trade.entry.quantity),
    )
}

fn time_in_force(entry: &Order) -> TimeInForce {
    match entry.time_in_force {
        trust_model::TimeInForce::Day => TimeInForce::Day,
        trust_model::TimeInForce::UntilCanceled => TimeInForce::UntilCanceled,
        trust_model::TimeInForce::UntilMarketClose => TimeInForce::UntilMarketClose,
        trust_model::TimeInForce::UntilMarketOpen => TimeInForce::UntilMarketOpen,
    }
}

fn side(trade: &Trade) -> Side {
    match trade.category {
        TradeCategory::Long => Side::Buy,
        TradeCategory::Short => Side::Sell,
    }
}
#[cfg(test)]
mod tests {
    use std::default;

    use rust_decimal_macros::dec;
    use trust_model::Price;

    use super::*;

    #[test]
    fn test_new_request() {
        // Create a sample trade object
        let trade = Trade {
            safety_stop: Order {
                unit_price: Price {
                    amount: dec!(10.27),
                    ..Default::default()
                },
                ..Default::default()
            },
            entry: Order {
                unit_price: Price {
                    amount: dec!(13.22),
                    ..Default::default()
                },
                ..Default::default()
            },
            target: Order {
                unit_price: Price {
                    amount: dec!(15.03),
                    ..Default::default()
                },
                ..Default::default()
            },
            ..Default::default()
        };

        // Call the new_request function with the sample trade object
        let order_req = new_request(&trade);

        // Check if the returned OrderReq object has the correct values
        assert_eq!(order_req.class, Class::Bracket);
        assert_eq!(order_req.type_, Type::Limit);
        assert_eq!(
            order_req.limit_price.unwrap(),
            Num::from(trade.entry.unit_price.amount.to_u128().unwrap())
        );
        assert_eq!(
            order_req.take_profit.unwrap(),
            TakeProfit::Limit(Num::from(trade.target.unit_price.amount.to_u128().unwrap()))
        );
        assert_eq!(
            order_req.stop_loss.unwrap(),
            StopLoss::Stop(Num::from(
                trade.safety_stop.unit_price.amount.to_u128().unwrap()
            ))
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
}
