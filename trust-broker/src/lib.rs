use apca::api::v2::order::{
    Amount, Class, Order, OrderReq, OrderReqInit, Post, Side, StopLoss, TakeProfit, Type,
};
use apca::ApiInfo;
use apca::Client;
use num_decimal::Num;
use rust_decimal::prelude::ToPrimitive;
use tokio::runtime::Runtime; // 0.3.5
use uuid::Uuid;

use std::error::Error;
use trust_model::{Broker, BrokerLog, Trade};

pub struct AlpacaBroker {
    api_base_url: String,
    key_id: String,
    secret: String,
}

impl Broker for AlpacaBroker {
    fn submit_order(self, trade: &Trade) -> Result<BrokerLog, Box<dyn Error>> {
        let api_info = ApiInfo::from_parts(self.api_base_url, self.key_id, self.secret).unwrap();
        let client = Client::new(api_info);

        let request = OrderReqInit {
            class: Class::Bracket,
            type_: Type::Limit,
            limit_price: Some(Num::from(184)),
            take_profit: Some(TakeProfit::Limit(Num::from(185))),
            stop_loss: Some(StopLoss::Stop(Num::from(178))),
            ..Default::default()
        }
        .init("AAPL", Side::Buy, Amount::quantity(1));

        let order = Runtime::new().unwrap().block_on(submit(client, request))?;

        Ok(new_log(trade, format!("{:?}", order)))
    }
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
        log: log,
    }
}

fn new_request(trade: &Trade) -> OrderReq {
    let request = OrderReqInit {
        class: Class::Bracket,
        type_: Type::Limit,
        limit_price: Some(Num::from(trade.entry.unit_price.amount.to_u128().unwrap())),
        take_profit: Some(TakeProfit::Limit(Num::from(185))),
        stop_loss: Some(StopLoss::Stop(Num::from(178))),
        ..Default::default()
    }
    .init("AAPL", Side::Buy, Amount::quantity(1));
    request
}
