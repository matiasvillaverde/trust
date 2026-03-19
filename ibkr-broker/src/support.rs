use model::{Account, Trade};
use std::error::Error;

pub(crate) fn broker_account_id(account: &Account) -> Result<&str, Box<dyn Error>> {
    account
        .broker_account_id
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| {
            format!(
                "IBKR account '{}' is missing broker_account_id. Recreate or update the account with the IBKR acctId.",
                account.name
            )
            .into()
        })
}

pub(crate) fn ensure_trade_account(trade: &Trade, account: &Account) -> Result<(), Box<dyn Error>> {
    if trade.account_id == account.id {
        return Ok(());
    }
    Err("Trade account does not match the broker account".into())
}
