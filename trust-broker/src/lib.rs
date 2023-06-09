use apca::api::v2::order::{
    Amount, Class, Order as AlpacaOrder, OrderReq, OrderReqInit, Post, Side, StopLoss, TakeProfit,
    TimeInForce, Type,
};
use apca::ApiInfo;
use apca::Client;

use num_decimal::Num;
use std::str::FromStr;
use tokio::runtime::Runtime;
use uuid::Uuid;

use std::error::Error;
use trust_model::{
    Account, Broker, BrokerLog, Environment, Order, OrderIds, Status, Trade, TradeCategory,
};

mod keys;
mod sync;
pub use keys::Keys;

#[derive(Default)]
pub struct AlpacaBroker;

impl Broker for AlpacaBroker {
    fn submit_trade(
        &self,
        trade: &Trade,
        account: &Account,
    ) -> Result<(BrokerLog, OrderIds), Box<dyn Error>> {
        assert!(trade.account_id == account.id); // Verify that the trade is for the account

        let api_info = read_api_key(&account.environment, account)?;
        let client = Client::new(api_info);

        let request = new_request(trade);
        let order = Runtime::new().unwrap().block_on(submit(client, request))?;

        let log = new_log(trade, format!("{:?}", order));
        let ids = extract_ids(order);
        Ok((log, ids))
    }

    fn sync_trade(
        &self,
        trade: &Trade,
        account: &Account,
    ) -> Result<(Status, Vec<Order>), Box<dyn Error>> {
        assert!(trade.account_id == account.id); // Verify that the trade is for the account

        let api_info = read_api_key(&account.environment, account)?;
        let client = Client::new(api_info);

        Runtime::new()
            .unwrap()
            .block_on(sync::sync_trade(&client, &trade))
    }
}

impl AlpacaBroker {
    pub fn setup_keys(
        key_id: &str,
        secret: &str,
        url: &str,
        environment: &Environment,
        account: &Account,
    ) -> Result<Keys, Box<dyn Error>> {
        let keys = Keys::new(key_id, secret, url);
        let keys = keys.store(environment, &account.name)?;
        Ok(keys)
    }

    pub fn read_keys(environment: &Environment, account: &Account) -> Result<Keys, Box<dyn Error>> {
        let keys = Keys::read(environment, &account.name)?;
        Ok(keys)
    }

    pub fn delete_keys(environment: &Environment, account: &Account) -> Result<(), Box<dyn Error>> {
        Keys::delete(environment, &account.name)?;
        Ok(())
    }
}

fn read_api_key(env: &Environment, account: &Account) -> Result<ApiInfo, Box<dyn Error>> {
    let keys = Keys::read(env, &account.name)?;
    let info = ApiInfo::from_parts(keys.url, keys.key_id, keys.secret)?;
    Ok(info)
}

async fn submit(
    client: Client,
    request: OrderReq,
) -> Result<apca::api::v2::order::Order, Box<dyn Error>> {
    let result = client.issue::<Post>(&request).await;

    match result {
        Ok(order) => Ok(order),
        Err(e) => {
            eprintln!("Error submitting trade: {:?}. Are the US market open?", e);
            Err(Box::new(e))
        }
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

fn extract_ids(order: AlpacaOrder) -> OrderIds {
    let mut stop_id = Uuid::new_v4();
    let mut target_id = Uuid::new_v4();
    for leg in order.legs {
        if order.type_ == Type::Market {
            stop_id = Uuid::from_str(leg.id.to_string().as_str()).unwrap();
        } else if order.type_ == Type::Limit {
            target_id = Uuid::from_str(leg.id.to_string().as_str()).unwrap();
        }
    }
    let id: Uuid = Uuid::from_str(order.id.to_string().as_str()).unwrap();

    OrderIds {
        stop: stop_id,
        entry: id,
        target: target_id,
    }
}

fn new_request(trade: &Trade) -> OrderReq {
    let entry = Num::from_str(trade.entry.unit_price.amount.to_string().as_str()).unwrap();
    let stop = Num::from_str(trade.safety_stop.unit_price.amount.to_string().as_str()).unwrap();
    let target = Num::from_str(trade.target.unit_price.amount.to_string().as_str()).unwrap();

    OrderReqInit {
        class: Class::Bracket,
        type_: Type::Limit,
        limit_price: Some(entry),
        take_profit: Some(TakeProfit::Limit(target)),
        stop_loss: Some(StopLoss::Stop(stop)),
        time_in_force: time_in_force(&trade.entry),
        extended_hours: trade.entry.extended_hours,
        client_order_id: Some(trade.id.to_string()),
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
}
