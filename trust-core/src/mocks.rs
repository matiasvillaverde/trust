#[cfg(test)]
pub mod read_transaction_db_mocks {

    use chrono::Utc;
    use rust_decimal::Decimal;
    use std::error::Error;
    use trust_model::{
        Currency, Price, ReadTransactionDB, Trade, Transaction, TransactionCategory,
    };
    use uuid::Uuid;

    pub struct MockDatabase {
        account_id: Uuid,
        transactions: Vec<Transaction>,
    }

    impl MockDatabase {
        pub fn new() -> Self {
            MockDatabase {
                account_id: Uuid::new_v4(),
                transactions: Vec::new(),
            }
        }

        pub fn set_transaction(&mut self, category: TransactionCategory, amount: Decimal) {
            let now = Utc::now().naive_utc();
            let currency = Currency::USD;
            let transaction = Transaction {
                id: Uuid::new_v4(),
                created_at: now,
                updated_at: now,
                deleted_at: None,
                account_id: self.account_id,
                price: Price {
                    id: Uuid::new_v4(),
                    created_at: now,
                    updated_at: now,
                    deleted_at: None,
                    currency: currency,
                    amount,
                },
                currency: currency,
                category,
            };
            self.transactions.push(transaction);
        }
    }

    #[cfg(test)]
    impl ReadTransactionDB for MockDatabase {
        fn all_account_transactions_excluding_taxes(
            &mut self,
            _account_id: Uuid,
            _currency: &Currency,
        ) -> Result<Vec<Transaction>, Box<dyn Error>> {
            Ok(self.transactions.clone())
        }

        fn all_account_transactions_funding_in_open_trades(
            &mut self,
            _account_id: Uuid,
            _currency: &Currency,
        ) -> Result<Vec<Transaction>, Box<dyn Error>> {
            Ok(self.transactions.clone())
        }

        fn read_all_account_transactions_taxes(
            &mut self,
            _account_id: Uuid,
            _currency: &Currency,
        ) -> Result<Vec<Transaction>, Box<dyn Error>> {
            Ok(self.transactions.clone())
        }

        fn all_trade_transactions(
            &mut self,
            _trade: &Trade,
        ) -> Result<Vec<Transaction>, Box<dyn Error>> {
            Ok(self.transactions.clone())
        }

        fn all_trade_funding_transactions(
            &mut self,
            _trade: &Trade,
        ) -> Result<Vec<Transaction>, Box<dyn Error>> {
            Ok(self.transactions.clone())
        }

        fn all_trade_taxes_transactions(
            &mut self,
            _trade: &Trade,
        ) -> Result<Vec<Transaction>, Box<dyn Error>> {
            Ok(self.transactions.clone())
        }

        fn all_transaction_excluding_current_month_and_taxes(
            &mut self,
            _account_id: Uuid,
            _currency: &Currency,
        ) -> Result<Vec<Transaction>, Box<dyn Error>> {
            Ok(self.transactions.clone())
        }

        fn all_transactions(
            &mut self,
            _account_id: Uuid,
            _currency: &Currency,
        ) -> Result<Vec<Transaction>, Box<dyn Error>> {
            Ok(self.transactions.clone())
        }
    }
}
