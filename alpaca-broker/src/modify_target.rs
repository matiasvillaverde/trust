use crate::keys;
use apca::api::v2::order::{ChangeReqInit, Id, Order, Patch};
use apca::Client;
use model::BrokerLog;
use model::{Account, Trade};
use num_decimal::Num;
use rust_decimal::Decimal;
use std::{error::Error, str::FromStr};
use tokio::runtime::Runtime;
use uuid::Uuid;

pub fn modify(
    trade: &Trade,
    account: &Account,
    price: Decimal,
) -> Result<BrokerLog, Box<dyn Error>> {
    assert!(trade.account_id == account.id); // Verify that the trade is for the account

    let api_info = keys::read_api_key(&account.environment, account)?;
    let client = Client::new(api_info);

    // Modify the stop order.
    let alpaca_order = Runtime::new().unwrap().block_on(submit(
        &client,
        trade.target.broker_order_id.unwrap(),
        price,
    ))?;

    // 3. Log the Alpaca order.
    let log = BrokerLog {
        trade_id: trade.id,
        log: serde_json::to_string(&alpaca_order)?,
        ..Default::default()
    };

    Ok(log)
}

async fn submit(client: &Client, order_id: Uuid, price: Decimal) -> Result<Order, Box<dyn Error>> {
    let request = ChangeReqInit {
        limit_price: Some(Num::from_str(price.to_string().as_str()).unwrap()),
        ..Default::default()
    }
    .init();

    let result = client.issue::<Patch>(&(Id(order_id), request)).await;
    match result {
        Ok(log) => Ok(log),
        Err(e) => {
            eprintln!("Error modify stop: {:?}", e);
            Err(Box::new(e))
        }
    }
}
