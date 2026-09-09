use model::{Account, BrokerError, BrokerLog, Order, Trade};
use std::error::Error;

/// Alpaca cannot atomically convert a bracket target into a market exit.
/// Canceling either bracket leg also cancels its protective sibling. Fail before
/// credentials or network access rather than expose the position between requests.
pub fn close(trade: &Trade, account: &Account) -> Result<(Order, BrokerLog), Box<dyn Error>> {
    crate::ensure_trade_account(trade, account)?;
    Err(BrokerError::retry_safe_mutation(
        "close_trade",
        "Safe market close of an Alpaca bracket is unsupported. The bracket was left intact. Use an explicitly priced target replacement or manage the position at the broker, then sync.",
    ).into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn market_close_fails_before_any_external_mutation() {
        let account = Account::default();
        let trade = Trade {
            account_id: account.id,
            ..Trade::default()
        };
        let error = close(&trade, &account).unwrap_err();
        assert!(error
            .downcast_ref::<BrokerError>()
            .is_some_and(BrokerError::is_retry_safe_mutation));
        assert!(error.to_string().contains("bracket was left intact"));
    }

    #[test]
    fn mismatched_account_is_rejected() {
        let trade = Trade {
            account_id: uuid::Uuid::new_v4(),
            ..Trade::default()
        };
        assert!(close(&trade, &Account::default()).is_err());
    }
}
