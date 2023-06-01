#[cfg(test)]
pub mod read_transaction_db_mocks {

    use chrono::Utc;
    use rust_decimal::Decimal;
    use std::error::Error;
    use trust_model::{
        Currency, Order, OrderAction, OrderCategory, Price, ReadTradeDB, ReadTransactionDB, Target,
        Trade, TradeCategory, TradeOverview, TradingVehicle, Transaction, TransactionCategory,
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
                price: Price {
                    id: Uuid::new_v4(),
                    created_at: now,
                    updated_at: now,
                    deleted_at: None,
                    currency,
                    amount,
                },
                currency,
                category,
            };
            self.transactions.push(transaction);
        }

        pub fn set_trade(&mut self, entry: Decimal, target: Decimal, stop: Decimal) {
            let now: chrono::NaiveDateTime = Utc::now().naive_utc();

            let target = Target {
                id: Uuid::new_v4(),
                created_at: now,
                updated_at: now,
                deleted_at: None,
                target_price: Price::default(),
                order: MockDatabase::order(target, OrderCategory::Market, OrderAction::Sell),
                trade_id: Uuid::new_v4(),
            };

            let trade = Trade {
                id: Uuid::new_v4(),
                created_at: now,
                updated_at: now,
                deleted_at: None,
                trading_vehicle: TradingVehicle::default(),
                currency: Currency::USD,
                safety_stop: MockDatabase::order(stop, OrderCategory::Stop, OrderAction::Sell),
                entry: MockDatabase::order(entry, OrderCategory::Limit, OrderAction::Buy),
                exit_targets: vec![target],
                category: TradeCategory::Long,
                account_id: self.account_id,
                approved_at: None,
                rejected_at: None,
                opened_at: None,
                failed_at: None,
                closed_at: None,
                rejected_by_rule_id: None,
                overview: TradeOverview::default(),
            };

            self.trades.push(trade);
        }

        fn order(amount: Decimal, category: OrderCategory, action: OrderAction) -> Order {
            let now: chrono::NaiveDateTime = Utc::now().naive_utc();
            Order {
                id: Uuid::new_v4(),
                created_at: now,
                updated_at: now,
                deleted_at: None,
                unit_price: Price::default(),
                quantity: 10,
                trading_vehicle_id: Uuid::new_v4(),
                category: category,
                action: action,
                opened_at: None,
                closed_at: None,
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

        fn all_account_transactions_funding_in_approved_trades(
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
            account_id: Uuid,
            currency: &Currency,
        ) -> Result<Vec<Trade>, Box<dyn Error>> {
            Ok(self.trades.clone())
        }

        fn all_approved_trades(&mut self, account_id: Uuid) -> Result<Vec<Trade>, Box<dyn Error>> {
            Ok(self.trades.clone())
        }

        fn all_open_trades(&mut self, account_id: Uuid) -> Result<Vec<Trade>, Box<dyn Error>> {
            Ok(self.trades.clone())
        }

        fn read_trade(&mut self, id: Uuid) -> Result<Trade, Box<dyn Error>> {
            Ok(self.trades.first().unwrap().clone())
        }

        fn read_all_new_trades(&mut self, account_id: Uuid) -> Result<Vec<Trade>, Box<dyn Error>> {
            Ok(self.trades.clone())
        }
    }
}
