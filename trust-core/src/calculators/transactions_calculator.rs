use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use trust_model::{Currency, DatabaseFactory, ReadTransactionDB, Trade, TransactionCategory};
use uuid::Uuid;

pub struct TradeTransactionsCalculator;

impl TradeTransactionsCalculator {
    pub fn capital_in_trades_not_at_risk(
        account_id: Uuid,
        currency: &Currency,
        database: &mut dyn DatabaseFactory,
    ) -> Result<Decimal, Box<dyn std::error::Error>> {
        // Get the capital of the open trades that is not at risk to the total available.
        let open_trades = database
            .read_trade_db()
            .all_open_trades_for_currency(account_id, currency)?;
        let mut total_capital_not_at_risk = dec!(0.0);

        for trade in open_trades {
            let risk_per_share =
                trade.entry.unit_price.amount - trade.safety_stop.unit_price.amount;
            let total_risk = risk_per_share * Decimal::from(trade.entry.quantity);
            total_capital_not_at_risk += total_risk;
        }
        Ok(total_capital_not_at_risk)
    }

    // Trade transactions

    pub fn taxes(
        trade: &Trade,
        database: &mut dyn ReadTransactionDB,
    ) -> Result<Decimal, Box<dyn std::error::Error>> {
        let mut total_trade = dec!(0);

        for tx in database.all_trade_taxes_transactions(trade.id)? {
            match tx.category {
                TransactionCategory::PaymentTax(_) => {
                    // This is money that we have used to enter the market.
                    total_trade += tx.price.amount
                }
                default => panic!(
                    "taxes: does not know how to calculate transaction with category: {}",
                    default
                ),
            }
        }

        Ok(total_trade)
    }

    pub fn total_performance(
        trade: &Trade,
        database: &mut dyn ReadTransactionDB,
    ) -> Result<Decimal, Box<dyn std::error::Error>> {
        let mut performance = dec!(0);

        for tx in database.all_trade_transactions(trade.id)? {
            match tx.category {
                TransactionCategory::OpenTrade(_)
                | TransactionCategory::FeeClose(_)
                | TransactionCategory::FeeOpen(_)
                | TransactionCategory::PaymentTax(_) => performance -= tx.price.amount,

                TransactionCategory::CloseTarget(_)
                | TransactionCategory::CloseSafetyStop(_)
                | TransactionCategory::CloseSafetyStopSlippage(_) => performance += tx.price.amount,
                _ => {} // We don't want to count the transactions paid out of the trade or fund the trade.
            }
        }

        Ok(performance)
    }
}

#[cfg(test)]
mod tests {}
