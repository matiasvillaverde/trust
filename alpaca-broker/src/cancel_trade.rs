use crate::keys;
use apca::api::v2::order::{Delete, Id};
use apca::Client;
use model::{Account, Trade};
use std::error::Error;
use tokio::runtime::Runtime;
use uuid::Uuid;

pub fn cancel(trade: &Trade, account: &Account) -> Result<(), Box<dyn Error>> {
    assert!(trade.account_id == account.id); // Verify that the trade is for the account

    // Validate required input before touching keychain/network.
    let broker_order_id = trade
        .entry
        .broker_order_id
        .ok_or("Entry order ID is missing")?;

    let api_info = keys::read_api_key(&account.environment, account)?;
    let client = Client::new(api_info);

    println!(
        "Canceling trade entry order: {:?}",
        trade.entry.broker_order_id
    );

    Runtime::new()
        .map_err(|e| Box::new(e) as Box<dyn Error>)?
        .block_on(cancel_entry(&client, broker_order_id))?;

    Ok(())
}

async fn cancel_entry(client: &Client, order_id: Uuid) -> Result<(), Box<dyn Error>> {
    let result = client.issue::<Delete>(&Id(order_id)).await;
    match result {
        Ok(_) => Ok(()),
        Err(e) => {
            eprintln!("Error cancel entry: {e:?}");
            Err(Box::new(e))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::cancel;
    use model::{Account, Trade};
    use uuid::Uuid;

    #[test]
    fn cancel_returns_error_when_entry_broker_order_id_is_missing() {
        let account = Account::default();
        let trade = Trade {
            account_id: account.id,
            ..Trade::default()
        };

        let err = cancel(&trade, &account).expect_err("missing order id should fail");
        assert!(err.to_string().contains("Entry order ID is missing"));
    }

    #[test]
    #[should_panic]
    fn cancel_panics_when_trade_account_mismatch() {
        let account = Account::default();
        let trade = Trade {
            account_id: Uuid::new_v4(),
            ..Trade::default()
        };

        let _ = cancel(&trade, &account);
    }
}
