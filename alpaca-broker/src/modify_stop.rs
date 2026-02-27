use crate::keys;
use apca::api::v2::order::{Change, ChangeReq, Id, Order};
use apca::Client;
use model::{Account, Trade};
use num_decimal::Num;
use rust_decimal::Decimal;
use std::{error::Error, str::FromStr};
use tokio::runtime::Runtime;
use uuid::Uuid;

pub fn modify(trade: &Trade, account: &Account, price: Decimal) -> Result<Uuid, Box<dyn Error>> {
    assert!(trade.account_id == account.id); // Verify that the trade is for the account

    // Validate required input before touching keychain/network.
    let stop_order_id = trade
        .safety_stop
        .broker_order_id
        .ok_or("Safety stop order ID is missing")?;

    let api_info = keys::read_api_key(&account.environment, account)?;
    let client = Client::new(api_info);

    let alpaca_order = Runtime::new()
        .map_err(|e| Box::new(e) as Box<dyn Error>)?
        .block_on(submit(&client, stop_order_id, price))?;

    Ok(alpaca_order.id.0)
}

async fn submit(client: &Client, order_id: Uuid, price: Decimal) -> Result<Order, Box<dyn Error>> {
    let request = ChangeReq {
        stop_price: Some(
            Num::from_str(&price.to_string())
                .map_err(|e| format!("Failed to parse stop price: {e:?}"))?,
        ),
        ..Default::default()
    };

    let result = client.issue::<Change>(&(Id(order_id), request)).await;
    match result {
        Ok(log) => Ok(log),
        Err(e) => {
            eprintln!("Error modify stop: {e:?}");
            Err(Box::new(e))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::modify;
    use model::{Account, Trade};
    use rust_decimal_macros::dec;
    use uuid::Uuid;

    #[test]
    fn modify_returns_error_when_stop_broker_order_id_is_missing() {
        let account = Account::default();
        let trade = Trade {
            account_id: account.id,
            ..Trade::default()
        };

        let err =
            modify(&trade, &account, dec!(100)).expect_err("missing stop order id should fail");
        assert!(err.to_string().contains("Safety stop order ID is missing"));
    }

    #[test]
    #[should_panic]
    fn modify_panics_when_trade_account_mismatch() {
        let account = Account::default();
        let trade = Trade {
            account_id: Uuid::new_v4(),
            ..Trade::default()
        };

        let _ = modify(&trade, &account, dec!(100));
    }
}
