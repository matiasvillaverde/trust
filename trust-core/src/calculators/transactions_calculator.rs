use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use trust_model::{Currency, DatabaseFactory, ReadTransactionDB, Trade, TransactionCategory};
use uuid::Uuid;

pub struct TransactionsCalculator;

impl TransactionsCalculator {
    pub fn capital_in_trades(
        account_id: Uuid,
        currency: &Currency,
        database: &mut dyn ReadTransactionDB,
    ) -> Result<Decimal, Box<dyn std::error::Error>> {
        // Get all transactions
        let transactions =
            database.all_account_transactions_funding_in_open_trades(account_id, currency)?;

        // Sum all transactions
        let total_available: Decimal = transactions
            .iter()
            .map(|transaction| match transaction.category {
                TransactionCategory::FundTrade(_) => transaction.price.amount,
                _ => dec!(0),
            })
            .sum();

        if total_available < dec!(0.0) {
            return Ok(dec!(0.0)); // If there is a negative meaning that we have a profit. So we return 0.
        }

        Ok(total_available)
    }

    pub fn capital_taxable(
        account_id: Uuid,
        currency: &Currency,
        database: &mut dyn ReadTransactionDB,
    ) -> Result<Decimal, Box<dyn std::error::Error>> {
        // Get all transactions
        let transactions = database.read_all_account_transactions_taxes(account_id, currency)?;

        // Sum all transactions
        let total_available: Decimal = transactions
            .iter()
            .map(|transaction| match transaction.category {
                TransactionCategory::PaymentTax(_) => transaction.price.amount,
                TransactionCategory::WithdrawalTax => -transaction.price.amount,
                default => panic!(
                    "capital_taxable: does not know how to calculate transaction with category: {}",
                    default
                ),
            })
            .sum();

        Ok(total_available)
    }

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

    pub fn capital_at_beginning_of_month(
        account_id: Uuid,
        currency: &Currency,
        database: &mut dyn ReadTransactionDB,
    ) -> Result<Decimal, Box<dyn std::error::Error>> {
        // Calculate all the transactions at the beginning of the month
        let mut total_beginning_of_month = dec!(0.0);
        for transaction in
            database.all_transaction_excluding_current_month_and_taxes(account_id, currency)?
        {
            match transaction.category {
                TransactionCategory::FundTrade(_)
                | TransactionCategory::Withdrawal
                | TransactionCategory::FeeOpen(_)
                | TransactionCategory::FeeClose(_) => {
                    total_beginning_of_month -= transaction.price.amount
                }
                TransactionCategory::PaymentFromTrade(_) => {
                    total_beginning_of_month += transaction.price.amount
                }
                TransactionCategory::Deposit => {
                    total_beginning_of_month += transaction.price.amount
                }
                default => panic!(
                    "capital_at_beginning_of_month: does not know how to calculate transaction with category: {}. Transaction: {:?}",
                    default,
                    transaction
                ),
            }
        }
        Ok(total_beginning_of_month)
    }

    // Trade transactions

    pub fn capital_out_of_market(
        trade: &Trade,
        database: &mut dyn ReadTransactionDB,
    ) -> Result<Decimal, Box<dyn std::error::Error>> {
        let mut total_trade = dec!(0);

        for tx in database.all_trade_transactions(trade)? {
            match tx.category {
                TransactionCategory::FundTrade(_) => {
                    // This is money that we have put into the trade
                    total_trade += tx.price.amount
                }
                TransactionCategory::PaymentFromTrade(_) => {
                    // This is money that we have extracted from the trade
                    total_trade -= tx.price.amount
                }
                TransactionCategory::OpenTrade(_) => {
                    // This is money that we have used to enter the market.
                    total_trade -= tx.price.amount
                }
                TransactionCategory::CloseTarget(_) => {
                    // This is money that we have used to exit the market.
                    total_trade += tx.price.amount
                }
                TransactionCategory::CloseSafetyStop(_) => {
                    // This is money that we have used to exit the market at a loss.
                    total_trade += tx.price.amount
                }
                TransactionCategory::CloseSafetyStopSlippage(_) => {
                    // This is money that we have used to exit the market at a loss - slippage.
                    total_trade += tx.price.amount
                },
                TransactionCategory::FeeOpen(_) | TransactionCategory::FeeClose(_)  => {
                    // We ignore the fees because they are charged from the account and not from the trade.
                }
                default => panic!(
                    "capital_out_of_market: does not know how to calculate transaction with category: {}",
                    default
                ),
            }
        }

        Ok(total_trade)
    }

    pub fn capital_in_market(
        trade: &Trade,
        database: &mut dyn ReadTransactionDB,
    ) -> Result<Decimal, Box<dyn std::error::Error>> {
        let mut total_trade = dec!(0);

        for tx in database.all_trade_transactions(trade)? {
            match tx.category {
                TransactionCategory::FundTrade(_) | TransactionCategory::PaymentFromTrade(_) => {
                    // Nothing
                }
                TransactionCategory::OpenTrade(_) => {
                    // This is money that we have used to enter the market.
                    total_trade += tx.price.amount
                }
                TransactionCategory::CloseTarget(_)
                | TransactionCategory::CloseSafetyStop(_)
                | TransactionCategory::CloseSafetyStopSlippage(_) => {
                    total_trade = Decimal::from(0) // We have exited the market, so we have no money in the market.
                },
                TransactionCategory::FeeOpen(_) | TransactionCategory::FeeClose(_)  => {
                    // We ignore the fees because they are charged from the account and not from the trade.
                }
                default => panic!(
                    "capital_in_market: does not know how to calculate transaction with category: {}",
                    default
                ),
            }
        }

        Ok(total_trade)
    }

    pub fn funding(
        trade: &Trade,
        database: &mut dyn ReadTransactionDB,
    ) -> Result<Decimal, Box<dyn std::error::Error>> {
        let mut total_trade = dec!(0);

        for tx in database.all_trade_funding_transactions(trade)? {
            match tx.category {
                TransactionCategory::FundTrade(_) => {
                    // This is money that we have used to enter the market.
                    total_trade += tx.price.amount
                }
                default => panic!(
                    "funding: does not know how to calculate transaction with category: {}",
                    default
                ),
            }
        }

        Ok(total_trade)
    }

    pub fn taxes(
        trade: &Trade,
        database: &mut dyn ReadTransactionDB,
    ) -> Result<Decimal, Box<dyn std::error::Error>> {
        let mut total_trade = dec!(0);

        for tx in database.all_trade_taxes_transactions(trade)? {
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

        for tx in database.all_trade_transactions(trade)? {
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
