#[cfg(test)]
pub mod read_transaction_db_mocks {

    use chrono::Utc;
    use rust_decimal::Decimal;
    use std::error::Error;
    use trust_model::{
        Currency, Order, OrderAction, OrderCategory, ReadTradeDB, ReadTransactionDB, Status, Trade,
        TradeCategory, TradeOverview, TradingVehicle, Transaction, TransactionCategory,
    };
    use uuid::Uuid;

    pub struct MockDatabase {
        account_id: Uuid,
        transactions: Vec<Transaction>,
        trades: Vec<Trade>,
    }

    impl MockDatabase {
        pub fn new() -> Self {
            MockDatabase {
                account_id: Uuid::new_v4(),
                transactions: Vec::new(),
                trades: Vec::new(),
            }
        }

        pub fn set_transaction(&mut self, category: TransactionCategory, amount: Decimal) {
            let now: chrono::NaiveDateTime = Utc::now().naive_utc();
            let currency = Currency::USD;
            let transaction = Transaction {
                id: Uuid::new_v4(),
                created_at: now,
                updated_at: now,
                deleted_at: None,
                account_id: self.account_id,
                amount,
                currency,
                category,
            };
            self.transactions.push(transaction);
        }

        pub fn set_trade(&mut self, entry: Decimal, target: Decimal, stop: Decimal, quantity: u64) {
            let now: chrono::NaiveDateTime = Utc::now().naive_utc();

            let trade = Trade {
                id: Uuid::new_v4(),
                created_at: now,
                updated_at: now,
                deleted_at: None,
                currency: Currency::USD,
                status: Status::default(),
                trading_vehicle: TradingVehicle::default(),
                safety_stop: MockDatabase::order(
                    stop,
                    OrderCategory::Stop,
                    OrderAction::Sell,
                    quantity,
                ),
                entry: MockDatabase::order(entry, OrderCategory::Limit, OrderAction::Buy, quantity),
                target: MockDatabase::order(
                    target,
                    OrderCategory::Limit,
                    OrderAction::Sell,
                    quantity,
                ),
                category: TradeCategory::Long,
                account_id: self.account_id,
                overview: TradeOverview::default(),
            };

            self.trades.push(trade);
        }

        fn order(
            amount: Decimal,
            category: OrderCategory,
            action: OrderAction,
            quantity: u64,
        ) -> Order {
            Order {
                unit_price: amount,
                quantity,
                category,
                action,
                ..Default::default()
            }
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

        fn all_account_transactions_funding_in_submitted_trades(
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
            _trade_id: Uuid,
        ) -> Result<Vec<Transaction>, Box<dyn Error>> {
            Ok(self.transactions.clone())
        }

        fn all_trade_funding_transactions(
            &mut self,
            _trade_id: Uuid,
        ) -> Result<Vec<Transaction>, Box<dyn Error>> {
            Ok(self.transactions.clone())
        }

        fn all_trade_taxes_transactions(
            &mut self,
            _trade_id: Uuid,
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

    #[cfg(test)]
    impl ReadTradeDB for MockDatabase {
        fn all_open_trades_for_currency(
            &mut self,
            _account_id: Uuid,
            _currency: &Currency,
        ) -> Result<Vec<Trade>, Box<dyn Error>> {
            Ok(self.trades.clone())
        }

        fn read_trades_with_status(
            &mut self,
            _account_id: Uuid,
            _status: Status,
        ) -> Result<Vec<Trade>, Box<dyn Error>> {
            Ok(self.trades.clone())
        }

        fn read_trade(&mut self, _id: Uuid) -> Result<Trade, Box<dyn Error>> {
            Ok(self.trades.first().unwrap().clone())
        }
    }
}
