use crate::keys;
use apca::api::v2::order::{Change, ChangeReq, Id, Order};
use apca::Client;
use model::{Account, Trade};
use num_decimal::Num;
use rust_decimal::Decimal;
use std::{error::Error, str::FromStr};
use tokio::runtime::Runtime;
use uuid::Uuid;

pub fn modify(trade: &Trade, account: &Account, price: Decimal) -> Result<String, Box<dyn Error>> {
    crate::ensure_trade_account(trade, account)?;

    // Validate required input before touching keychain/network.
    let stop_order_id = trade
        .safety_stop
        .broker_order_id
        .as_deref()
        .ok_or("Safety stop order ID is missing")?;
    let stop_order_id = Uuid::parse_str(stop_order_id)
        .map_err(|e| format!("Safety stop order ID is not a valid UUID: {e}"))?;

    let api_info = keys::read_api_key(&account.environment, account)?;
    let client = Client::new(api_info);

    let alpaca_order = Runtime::new()
        .map_err(|e| Box::new(e) as Box<dyn Error>)?
        .block_on(submit(&client, stop_order_id, price))?;

    Ok(alpaca_order.id.0.to_string())
}

fn change_request(price: Decimal) -> Result<ChangeReq, Box<dyn Error>> {
    Ok(ChangeReq {
        stop_price: Some(
            Num::from_str(&price.to_string())
                .map_err(|e| format!("Failed to parse stop price: {e:?}"))?,
        ),
        ..Default::default()
    })
}

async fn submit(client: &Client, order_id: Uuid, price: Decimal) -> Result<Order, Box<dyn Error>> {
    let request = change_request(price)?;

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
    use super::{change_request, modify};
    use model::{Account, Trade};
    use num_decimal::Num;
    use rust_decimal_macros::dec;
    use std::str::FromStr;
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
    fn modify_returns_error_when_stop_broker_order_id_is_invalid() {
        let account = Account::default();
        let trade = Trade {
            account_id: account.id,
            safety_stop: model::Order {
                broker_order_id: Some("not-a-uuid".to_string()),
                ..Default::default()
            },
            ..Trade::default()
        };

        let err =
            modify(&trade, &account, dec!(100)).expect_err("invalid stop order id should fail");
        assert!(err
            .to_string()
            .contains("Safety stop order ID is not a valid UUID"));
    }

    #[test]
    fn modify_returns_error_when_trade_account_mismatch() {
        let account = Account::default();
        let trade = Trade {
            account_id: Uuid::new_v4(),
            ..Trade::default()
        };

        let err = modify(&trade, &account, dec!(100)).expect_err("account mismatch should fail");
        assert!(err
            .to_string()
            .contains("Trade account does not match broker account"));
    }

    #[test]
    fn change_request_sets_only_stop_price() {
        let request = change_request(dec!(123.45)).expect("valid decimal should build request");

        assert_eq!(
            request.stop_price,
            Some(Num::from_str("123.45").expect("valid num"))
        );
        assert_eq!(request.limit_price, None);
    }
}
