use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use trust_model::{Currency, Database, Trade, TransactionCategory};
use uuid::Uuid;

pub struct TransactionsCalculator;

impl TransactionsCalculator {
    pub fn calculate_total_capital_available(
        account_id: Uuid,
        currency: &Currency,
        database: &mut dyn Database,
    ) -> Result<Decimal, Box<dyn std::error::Error>> {
        // Get all transactions
        let transactions = database.all_transactions_excluding_taxes(account_id, currency)?;

        // Sum all transactions
        let total_available: Decimal = transactions
            .iter()
            .map(|transaction| match transaction.category {
                TransactionCategory::FundTrade(_) | TransactionCategory::Withdrawal => {
                    -transaction.price.amount
                }
                TransactionCategory::PaymentFromTrade(_) | TransactionCategory::Deposit => {
                    transaction.price.amount
                }
                default => panic!("Unexpected transaction category: {}", default),
            })
            .sum();

        Ok(total_available)
    }

    pub fn total_capital_in_trades_not_at_risk(
        account_id: Uuid,
        currency: &Currency,
        database: &mut dyn Database,
    ) -> Result<Decimal, Box<dyn std::error::Error>> {
        // Get the capital of the open trades that is not at risk to the total available.
        let open_trades = database.all_open_trades_for_currency(account_id, currency)?;
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
        account_id: Uuid,
        currency: &Currency,
        database: &mut dyn Database,
    ) -> Result<Decimal, Box<dyn std::error::Error>> {
        // Calculate all the transactions at the beginning of the month
        let mut total_beginning_of_month = dec!(0.0);
        for transaction in
            database.all_transaction_excluding_current_month_and_taxes(account_id, currency)?
        {
            match transaction.category {
                TransactionCategory::FundTrade(_) => {
                    total_beginning_of_month -= transaction.price.amount
                }
                TransactionCategory::PaymentFromTrade(_) => {
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

    pub fn calculate_total_out_of_market_from(
        trade: &Trade,
        database: &mut dyn Database,
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
                }
                default => panic!("Unexpected transaction category: {}", default),
            }
        }

        Ok(total_trade)
    }

    pub fn calculate_total_in_market_from(
        trade: &Trade,
        database: &mut dyn Database,
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
                }
                default => panic!("Unexpected transaction category: {}", default),
            }
        }

        Ok(total_trade)
    }
}

// #[cfg(test)]
// mod tests {
//     use super::*;
//     use rust_decimal::Decimal;
//     use trust_model::{Account, Transaction};

//     #[test]
//     fn test_calculate_total_capital_available() {
//         // Create a mock database with some transactions
//         let mut db = MemoryDatabase::default();
//         let account = db.new_account("Test Account", "Description").unwrap();

//         deposit(dec!(10.0), &account, &mut db);
//         deposit(dec!(20.0), &account, &mut db);

//         // Calculate total capital available
//         let result = TransactionsCalculator::calculate_total_capital_available(
//             account.id,
//             &Currency::USD,
//             &mut db,
//         );

//         // Check that the result is correct
//         assert_eq!(result.unwrap(), dec!(30.0));

//         // Create a withdrawal
//         withdrawal(dec!(10.0), &account, &mut db);
//         withdrawal(dec!(5.0), &account, &mut db);

//         // Calculate total capital available
//         let result = TransactionsCalculator::calculate_total_capital_available(
//             account.id,
//             &Currency::USD,
//             &mut db,
//         );

//         // Check that the result is correct
//         assert_eq!(result.unwrap(), dec!(15.0));

//         // Create a trade in
//         trade(dec!(5.0), &account, &mut db);
//         trade(dec!(1.0), &account, &mut db);
//         output_tax(dec!(0.1), &account, &mut db); // This should not be included

//         // Calculate total capital available

//         let result = TransactionsCalculator::calculate_total_capital_available(
//             account.id,
//             &Currency::USD,
//             &mut db,
//         );

//         // Check that the result is correct
//         assert_eq!(result.unwrap(), dec!(9.0));

//         // Create a trade out

//         input_tax(dec!(0.1), &account, &mut db); // This should not be included
//         trade_payment(dec!(1.0), &account, &mut db);
//         trade_payment(dec!(2.0), &account, &mut db);

//         // Calculate total capital available
//         let result = TransactionsCalculator::calculate_total_capital_available(
//             account.id,
//             &Currency::USD,
//             &mut db,
//         );

//         // Check that the result is correct
//         assert_eq!(result.unwrap(), dec!(12.0));
//     }

//     fn deposit(amount: Decimal, account: &Account, db: &mut MemoryDatabase) -> Transaction {
//         db.new_transaction(
//             account,
//             amount,
//             &Currency::USD,
//             TransactionCategory::Deposit,
//         )
//         .unwrap()
//     }

//     fn withdrawal(amount: Decimal, account: &Account, db: &mut MemoryDatabase) -> Transaction {
//         db.new_transaction(
//             account,
//             amount,
//             &Currency::USD,
//             TransactionCategory::Withdrawal,
//         )
//         .unwrap()
//     }

//     fn trade(amount: Decimal, account: &Account, db: &mut MemoryDatabase) -> Transaction {
//         db.new_transaction(
//             account,
//             amount,
//             &Currency::USD,
//             TransactionCategory::FundTrade(Uuid::new_v4()),
//         )
//         .unwrap()
//     }

//     fn trade_payment(amount: Decimal, account: &Account, db: &mut MemoryDatabase) -> Transaction {
//         db.new_transaction(
//             account,
//             amount,
//             &Currency::USD,
//             TransactionCategory::PaymentFromTrade(Uuid::new_v4()),
//         )
//         .unwrap()
//     }

//     fn input_tax(amount: Decimal, account: &Account, db: &mut MemoryDatabase) -> Transaction {
//         db.new_transaction(
//             account,
//             amount,
//             &Currency::USD,
//             TransactionCategory::PaymentTax(Uuid::new_v4()),
//         )
//         .unwrap()
//     }

//     fn output_tax(amount: Decimal, account: &Account, db: &mut MemoryDatabase) -> Transaction {
//         db.new_transaction(
//             account,
//             amount,
//             &Currency::USD,
//             TransactionCategory::WithdrawalTax,
//         )
//         .unwrap()
//     }
// }
