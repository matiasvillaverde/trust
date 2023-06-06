use apca::api::v2::order::{
    Amount, Class, Order, OrderReq, OrderReqInit, Post, Side, StopLoss, TakeProfit, Type,
};
use apca::ApiInfo;
use apca::Client;

use num_decimal::Num;
use rust_decimal::prelude::ToPrimitive;
use tokio::runtime::Runtime;
use uuid::Uuid;

use std::error::Error;
use trust_model::{Broker, BrokerLog, Trade, TradeCategory};

pub struct AlpacaBroker;

impl Broker for AlpacaBroker {
    fn submit_order(self, trade: &Trade) -> Result<BrokerLog, Box<dyn Error>> {
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

async fn submit(client: Client, request: OrderReq) -> Result<Order, Box<dyn Error>> {
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
        take_profit: Some(TakeProfit::Limit(stop)),
        stop_loss: Some(StopLoss::Stop(target)),
        ..Default::default()
    }
    .init(
        trade.trading_vehicle.symbol.to_uppercase(),
        side(trade),
        Amount::quantity(trade.entry.quantity),
    )
}

fn side(trade: &Trade) -> Side {
    match trade.category {
        TradeCategory::Long => Side::Buy,
        TradeCategory::Short => Side::Sell,
    }
}
