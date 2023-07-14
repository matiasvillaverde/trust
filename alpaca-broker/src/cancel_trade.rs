use crate::keys;
use apca::api::v2::order::{Delete, Id};
use apca::Client;
use model::{Account, Trade};
use std::error::Error;
use tokio::runtime::Runtime;
use uuid::Uuid;

pub fn cancel(trade: &Trade, account: &Account) -> Result<(), Box<dyn Error>> {
    assert!(trade.account_id == account.id); // Verify that the trade is for the account

    let api_info = keys::read_api_key(&account.environment, account)?;
    let client = Client::new(api_info);

    println!(
        "Canceling trade entry order: {:?}",
        trade.entry.broker_order_id
    );

    // Cancel the entry order.
    Runtime::new()
        .unwrap()
        .block_on(cancel_entry(&client, trade.entry.broker_order_id.unwrap()))?;

    Ok(())
}

async fn cancel_entry(client: &Client, order_id: Uuid) -> Result<(), Box<dyn Error>> {
    let result = client.issue::<Delete>(&Id(order_id)).await;
    match result {
        Ok(_) => Ok(()),
        Err(e) => {
            eprintln!("Error cancel entry: {:?}", e);
            Err(Box::new(e))
        }
    }
}
