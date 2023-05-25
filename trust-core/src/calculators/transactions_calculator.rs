use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use trust_model::{Currency, Database, TransactionCategory};
use uuid::Uuid;

pub struct TransactionsCalculator;

impl TransactionsCalculator {
    pub fn calculate_total_capital_available(
        // TODO: Test this function
        account_id: Uuid,
        currency: &Currency,
        database: &mut dyn Database,
    ) -> Result<Decimal, Box<dyn std::error::Error>> {
        // Get all transactions
        let transactions =
            database.all_trade_transactions_excluding_taxes(account_id, &currency)?;
        let mut total_available = dec!(0.0);

        // Sum all transactions
        for transaction in transactions {
            match transaction.category {
                TransactionCategory::Output(_) => total_available -= transaction.price.amount,
                TransactionCategory::Input(_) => total_available += transaction.price.amount,
                TransactionCategory::Deposit => total_available += transaction.price.amount,
                TransactionCategory::Withdrawal => total_available -= transaction.price.amount,
                default => panic!("Unexpected transaction category: {}", default),
            }
        }

        Ok(total_available)
    }

    pub fn total_capital_in_trades_not_at_risk(
        // TODO: Test this function
        account_id: Uuid,
        currency: &Currency,
        database: &mut dyn Database,
    ) -> Result<Decimal, Box<dyn std::error::Error>> {
        // Get the capital of the open trades that is not at risk to the total available.
        let open_trades = database.all_open_trades(account_id, &currency)?;
        let mut total_capital_not_at_risk = dec!(0.0);

        for trade in open_trades {
            let risk_per_share =
                trade.entry.unit_price.amount - trade.safety_stop.unit_price.amount;
            let total_risk = risk_per_share * Decimal::from(trade.entry.quantity);
            total_capital_not_at_risk += total_risk;
        }
        Ok(total_capital_not_at_risk)
    }

    pub fn calculate_total_capital_at_beginning_of_month(
        // TODO: Test this function
        account_id: Uuid,
        currency: &Currency,
        database: &mut dyn Database,
    ) -> Result<Decimal, Box<dyn std::error::Error>> {
        // Calculate all the transactions at the beginning of the month
        let mut total_beginning_of_month = dec!(0.0);
        for transaction in
            database.all_transaction_excluding_current_month_and_taxes(account_id, &currency)?
        {
            match transaction.category {
                TransactionCategory::Output(_) => {
                    total_beginning_of_month -= transaction.price.amount
                }
                TransactionCategory::Input(_) => {
                    total_beginning_of_month += transaction.price.amount
                }
                TransactionCategory::Deposit => {
                    total_beginning_of_month += transaction.price.amount
                }
                TransactionCategory::Withdrawal => {
                    total_beginning_of_month -= transaction.price.amount
                }
                default => panic!("Unexpected transaction category: {}", default),
            }
        }
        Ok(total_beginning_of_month)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal::Decimal;
    // TODO: Add tests
}