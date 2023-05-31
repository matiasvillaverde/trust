use crate::workers::{
    AccountDB, WorkerAccountOverview, WorkerOrder, WorkerPrice, WorkerRule, WorkerTarget,
    WorkerTrade, WorkerTradingVehicle, WorkerTransaction,
};
use diesel::prelude::*;
use rust_decimal::Decimal;
use std::error::Error;
use std::sync::Arc;
use std::sync::Mutex;
use trust_model::{
    database::{WriteAccountDB, WriteTradeOverviewDB},
    Account, AccountOverview, Currency, DatabaseFactory, Order, OrderAction, OrderCategory, Price,
    ReadAccountDB, ReadAccountOverviewDB, ReadOrderDB, ReadPriceDB, ReadRuleDB, ReadTradeDB,
    ReadTradingVehicleDB, ReadTransactionDB, Rule, RuleName, Target, Trade, TradeCategory,
    TradeOverview, TradingVehicle, TradingVehicleCategory, Transaction, TransactionCategory,
    WriteAccountOverviewDB, WriteOrderDB, WritePriceDB, WriteRuleDB, WriteTradeDB,
    WriteTradingVehicleDB, WriteTransactionDB,
};
use uuid::Uuid;

pub struct SqliteDatabase {
    connection: Arc<Mutex<SqliteConnection>>,
}

impl DatabaseFactory for SqliteDatabase {
    fn read_account_db(&self) -> Box<dyn ReadAccountDB> {
        Box::new(AccountDB {
            connection: self.connection.clone(),
        })
    }

    fn write_account_db(&self) -> Box<dyn WriteAccountDB> {
        Box::new(AccountDB {
            connection: self.connection.clone(),
        })
    }

    fn read_account_overview_db(&self) -> Box<dyn ReadAccountOverviewDB> {
        Box::new(SqliteDatabase::new_from(self.connection.clone()))
    }

    fn write_account_overview_db(&self) -> Box<dyn WriteAccountOverviewDB> {
        Box::new(SqliteDatabase::new_from(self.connection.clone()))
    }
    fn read_order_db(&self) -> Box<dyn ReadOrderDB> {
        Box::new(SqliteDatabase::new_from(self.connection.clone()))
    }
    fn write_order_db(&self) -> Box<dyn WriteOrderDB> {
        Box::new(SqliteDatabase::new_from(self.connection.clone()))
    }
    fn read_price_db(&self) -> Box<dyn ReadPriceDB> {
        Box::new(SqliteDatabase::new_from(self.connection.clone()))
    }
    fn write_price_db(&self) -> Box<dyn WritePriceDB> {
        Box::new(SqliteDatabase::new_from(self.connection.clone()))
    }
    fn read_transaction_db(&self) -> Box<dyn ReadTransactionDB> {
        Box::new(SqliteDatabase::new_from(self.connection.clone()))
    }
    fn write_transaction_db(&self) -> Box<dyn WriteTransactionDB> {
        Box::new(SqliteDatabase::new_from(self.connection.clone()))
    }
    fn read_trade_db(&self) -> Box<dyn ReadTradeDB> {
        Box::new(SqliteDatabase::new_from(self.connection.clone()))
    }
    fn write_trade_db(&self) -> Box<dyn WriteTradeDB> {
        Box::new(SqliteDatabase::new_from(self.connection.clone()))
    }
    fn write_trade_overview_db(&self) -> Box<dyn WriteTradeOverviewDB> {
        Box::new(SqliteDatabase::new_from(self.connection.clone()))
    }
    fn read_rule_db(&self) -> Box<dyn ReadRuleDB> {
        Box::new(SqliteDatabase::new_from(self.connection.clone()))
    }
    fn write_rule_db(&self) -> Box<dyn WriteRuleDB> {
        Box::new(SqliteDatabase::new_from(self.connection.clone()))
    }
    fn read_trading_vehicle_db(&self) -> Box<dyn ReadTradingVehicleDB> {
        Box::new(SqliteDatabase::new_from(self.connection.clone()))
    }
    fn write_trading_vehicle_db(&self) -> Box<dyn WriteTradingVehicleDB> {
        Box::new(SqliteDatabase::new_from(self.connection.clone()))
    }
}

impl SqliteDatabase {
    pub fn new(url: &str) -> Self {
        let connection: SqliteConnection = Self::establish_connection(url);
        SqliteDatabase {
            connection: Arc::new(Mutex::new(connection)),
        }
    }

    pub fn new_from(connection: Arc<Mutex<SqliteConnection>>) -> Self {
        SqliteDatabase { connection }
    }

    #[doc(hidden)]
    pub fn new_in_memory() -> Self {
        use diesel_migrations::*;
        pub const MIGRATIONS: EmbeddedMigrations = embed_migrations!();
        let mut connection = SqliteConnection::establish(":memory:").unwrap();
        connection.run_pending_migrations(MIGRATIONS).unwrap();
        connection.begin_test_transaction().unwrap();
        SqliteDatabase {
            connection: Arc::new(Mutex::new(connection)),
        }
    }

    /// Establish a connection to the SQLite database.
    fn establish_connection(database_url: &str) -> SqliteConnection {
        let db_exists = std::path::Path::new(database_url).exists();
        // Use the database URL to establish a connection to the SQLite database
        let mut connection = SqliteConnection::establish(database_url)
            .unwrap_or_else(|_| panic!("Error connecting to {}", database_url));

        // Run migrations only if it is a new DB
        if !db_exists {
            use diesel_migrations::*;
            pub const MIGRATIONS: EmbeddedMigrations = embed_migrations!();
            connection.run_pending_migrations(MIGRATIONS).unwrap();
        }

        connection
    }
}

impl WriteOrderDB for SqliteDatabase {
    fn create_order(
        &mut self,
        trading_vehicle: &TradingVehicle,
        quantity: i64,
        price: Decimal,
        currency: &Currency,
        action: &OrderAction,
    ) -> Result<Order, Box<dyn Error>> {
        WorkerOrder::create(
            &mut self.connection.lock().unwrap(),
            price,
            currency,
            quantity,
            action,
            &OrderCategory::Market, // All stops should be market to go out as fast as possible
            trading_vehicle,
        )
    }

    fn create_target(
        &mut self,
        price: Decimal,
        currency: &Currency,
        order: &Order,
        trade: &Trade,
    ) -> Result<Target, Box<dyn Error>> {
        WorkerTarget::create(
            &mut self.connection.lock().unwrap(),
            price,
            currency,
            order,
            trade,
        )
    }

    fn record_order_opening(&mut self, order: &Order) -> Result<Order, Box<dyn Error>> {
        WorkerOrder::update_opened_at(&mut self.connection.lock().unwrap(), order)
    }

    fn record_order_closing(&mut self, order: &Order) -> Result<Order, Box<dyn Error>> {
        WorkerOrder::update_closed_at(&mut self.connection.lock().unwrap(), order)
    }
}

impl ReadAccountOverviewDB for SqliteDatabase {
    fn read_account_overview(
        &mut self,
        account_id: Uuid,
    ) -> Result<Vec<AccountOverview>, Box<dyn Error>> {
        WorkerAccountOverview::read(&mut self.connection.lock().unwrap(), account_id)
    }

    fn read_account_overview_currency(
        &mut self,
        account_id: Uuid,
        currency: &Currency,
    ) -> Result<AccountOverview, Box<dyn Error>> {
        WorkerAccountOverview::read_for_currency(
            &mut self.connection.lock().unwrap(),
            account_id,
            currency,
        )
    }
}

impl WriteAccountOverviewDB for SqliteDatabase {
    fn new_account_overview(
        &mut self,
        account: &Account,
        currency: &Currency,
    ) -> Result<AccountOverview, Box<dyn Error>> {
        let overview =
            WorkerAccountOverview::create(&mut self.connection.lock().unwrap(), account, currency)?;
        Ok(overview)
    }

    fn update_account_overview(
        &mut self,
        account: &Account,
        currency: &Currency,
        total_balance: Decimal,
        total_in_trade: Decimal,
        total_available: Decimal,
        taxed: Decimal,
    ) -> Result<AccountOverview, Box<dyn Error>> {
        let overview = WorkerAccountOverview::read_for_currency(
            &mut self.connection.lock().unwrap(),
            account.id,
            currency,
        )?;
        let updated_overview = WorkerAccountOverview::update_total_available(
            &mut self.connection.lock().unwrap(),
            overview,
            total_available,
        )?;

        let updated_overview = WorkerAccountOverview::update_total_in_trade(
            &mut self.connection.lock().unwrap(),
            updated_overview,
            total_in_trade,
        )?;

        let updated_overview = WorkerAccountOverview::update_total_balance(
            &mut self.connection.lock().unwrap(),
            updated_overview,
            total_balance,
        )?;

        let updated_overview = WorkerAccountOverview::update_taxed(
            &mut self.connection.lock().unwrap(),
            updated_overview,
            taxed,
        )?;

        Ok(updated_overview)
    }
}

impl WritePriceDB for SqliteDatabase {
    fn create_price(
        &mut self,
        currency: &Currency,
        amount: rust_decimal::Decimal,
    ) -> Result<Price, Box<dyn Error>> {
        WorkerPrice::create(&mut self.connection.lock().unwrap(), currency, amount)
    }
}

impl ReadPriceDB for SqliteDatabase {
    fn read_price(&mut self, id: uuid::Uuid) -> Result<Price, Box<dyn Error>> {
        WorkerPrice::read(&mut self.connection.lock().unwrap(), id)
    }
}

impl WriteTransactionDB for SqliteDatabase {
    fn create_transaction(
        &mut self,
        account: &Account,
        amount: rust_decimal::Decimal,
        currency: &Currency,
        category: TransactionCategory,
    ) -> Result<Transaction, Box<dyn Error>> {
        WorkerTransaction::create_transaction(
            &mut self.connection.lock().unwrap(),
            account.id,
            amount,
            currency,
            category,
        )
    }
}

impl ReadTransactionDB for SqliteDatabase {
    fn all_account_transactions_excluding_taxes(
        &mut self,
        account_id: Uuid,
        currency: &Currency,
    ) -> Result<Vec<Transaction>, Box<dyn Error>> {
        WorkerTransaction::read_all_trade_transactions_excluding_taxes(
            &mut self.connection.lock().unwrap(),
            account_id,
            currency,
        )
    }

    fn all_account_transactions_funding_in_approved_trades(
        &mut self,
        account_id: Uuid,
        currency: &Currency,
    ) -> Result<Vec<Transaction>, Box<dyn Error>> {
        WorkerTransaction::all_account_transactions_funding_in_approved_trades(
            &mut self.connection.lock().unwrap(),
            account_id,
            currency,
        )
    }

    fn read_all_account_transactions_taxes(
        &mut self,
        account_id: Uuid,
        currency: &Currency,
    ) -> Result<Vec<Transaction>, Box<dyn Error>> {
        WorkerTransaction::read_all_account_transactions_taxes(
            &mut self.connection.lock().unwrap(),
            account_id,
            currency,
        )
    }

    fn all_trade_transactions(
        &mut self,
        trade: &Trade,
    ) -> Result<Vec<Transaction>, Box<dyn Error>> {
        WorkerTransaction::read_all_trade_transactions(
            &mut self.connection.lock().unwrap(),
            trade.id,
        )
    }

    fn all_trade_funding_transactions(
        &mut self,
        trade: &Trade,
    ) -> Result<Vec<Transaction>, Box<dyn Error>> {
        WorkerTransaction::read_all_trade_transactions_for_category(
            &mut self.connection.lock().unwrap(),
            trade.id,
            TransactionCategory::FundTrade(trade.id),
        )
    }

    fn all_trade_taxes_transactions(
        &mut self,
        trade: &Trade,
    ) -> Result<Vec<Transaction>, Box<dyn Error>> {
        WorkerTransaction::read_all_trade_transactions_for_category(
            &mut self.connection.lock().unwrap(),
            trade.id,
            TransactionCategory::PaymentTax(trade.id),
        )
    }

    fn all_transaction_excluding_current_month_and_taxes(
        &mut self,
        account_id: Uuid,
        currency: &Currency,
    ) -> Result<Vec<Transaction>, Box<dyn Error>> {
        WorkerTransaction::read_all_transaction_excluding_current_month_and_taxes(
            &mut self.connection.lock().unwrap(),
            account_id,
            currency,
        )
    }

    fn all_transactions(
        &mut self,
        account_id: Uuid,
        currency: &Currency,
    ) -> Result<Vec<Transaction>, Box<dyn Error>> {
        WorkerTransaction::read_all_transactions(
            &mut self.connection.lock().unwrap(),
            account_id,
            currency,
        )
    }
}

impl ReadRuleDB for SqliteDatabase {
    fn read_all_rules(&mut self, account_id: Uuid) -> Result<Vec<Rule>, Box<dyn Error>> {
        WorkerRule::read_all(&mut self.connection.lock().unwrap(), account_id)
    }

    fn rule_for_account(
        &mut self,
        account_id: Uuid,
        name: &RuleName,
    ) -> Result<Rule, Box<dyn Error>> {
        WorkerRule::read_for_account_with_name(
            &mut self.connection.lock().unwrap(),
            account_id,
            name,
        )
    }
}

impl WriteRuleDB for SqliteDatabase {
    fn create_rule(
        &mut self,
        account: &Account,
        name: &trust_model::RuleName,
        description: &str,
        priority: u32,
        level: &trust_model::RuleLevel,
    ) -> Result<trust_model::Rule, Box<dyn Error>> {
        WorkerRule::create(
            &mut self.connection.lock().unwrap(),
            name,
            description,
            priority,
            level,
            account,
        )
    }

    fn make_rule_inactive(&mut self, rule: &Rule) -> Result<Rule, Box<dyn Error>> {
        WorkerRule::make_inactive(&mut self.connection.lock().unwrap(), rule)
    }
}

impl WriteTradingVehicleDB for SqliteDatabase {
    fn create_trading_vehicle(
        &mut self,
        symbol: &str,
        isin: &str,
        category: &TradingVehicleCategory,
        broker: &str,
    ) -> Result<TradingVehicle, Box<dyn Error>> {
        WorkerTradingVehicle::create(
            &mut self.connection.lock().unwrap(),
            symbol,
            isin,
            category,
            broker,
        )
    }
}

impl ReadTradingVehicleDB for SqliteDatabase {
    fn read_all_trading_vehicles(&mut self) -> Result<Vec<TradingVehicle>, Box<dyn Error>> {
        WorkerTradingVehicle::read_all(&mut self.connection.lock().unwrap())
    }

    fn read_trading_vehicle(&mut self, id: Uuid) -> Result<TradingVehicle, Box<dyn Error>> {
        WorkerTradingVehicle::read(&mut self.connection.lock().unwrap(), id)
    }
}

impl WriteTradeDB for SqliteDatabase {
    fn create_trade(
        &mut self,
        category: &TradeCategory,
        currency: &Currency,
        trading_vehicle: &TradingVehicle,
        safety_stop: &Order,
        entry: &Order,
        account: &Account,
    ) -> Result<Trade, Box<dyn Error>> {
        WorkerTrade::create(
            &mut self.connection.lock().unwrap(),
            category,
            currency,
            trading_vehicle,
            safety_stop,
            entry,
            account,
        )
    }

    fn approve_trade(&mut self, trade: &Trade) -> Result<Trade, Box<dyn Error>> {
        WorkerTrade::approve_trade(&mut self.connection.lock().unwrap(), trade)
    }

    fn update_trade_opened_at(&mut self, trade: &Trade) -> Result<Trade, Box<dyn Error>> {
        WorkerTrade::update_opened_at(&mut self.connection.lock().unwrap(), trade)
    }

    fn update_trade_closed_at(&mut self, trade: &Trade) -> Result<Trade, Box<dyn Error>> {
        WorkerTrade::update_closed_at(&mut self.connection.lock().unwrap(), trade)
    }
}

impl ReadTradeDB for SqliteDatabase {
    fn read_trade(&mut self, id: Uuid) -> Result<Trade, Box<dyn Error>> {
        WorkerTrade::read_trade(&mut self.connection.lock().unwrap(), id)
    }

    fn read_all_new_trades(&mut self, account_id: Uuid) -> Result<Vec<Trade>, Box<dyn Error>> {
        WorkerTrade::read_all_new_trades(&mut self.connection.lock().unwrap(), account_id)
    }

    fn all_open_trades_for_currency(
        &mut self,
        account_id: Uuid,
        currency: &Currency,
    ) -> Result<Vec<Trade>, Box<dyn Error>> {
        WorkerTrade::read_all_approved_trades_for_currency(
            &mut self.connection.lock().unwrap(),
            account_id,
            currency,
        )
    }

    fn all_approved_trades(&mut self, account_id: Uuid) -> Result<Vec<Trade>, Box<dyn Error>> {
        WorkerTrade::read_all_approved_trades(&mut self.connection.lock().unwrap(), account_id)
    }

    fn all_open_trades(&mut self, account_id: Uuid) -> Result<Vec<Trade>, Box<dyn Error>> {
        WorkerTrade::read_all_open_trades(&mut self.connection.lock().unwrap(), account_id)
    }
}

impl WriteTradeOverviewDB for SqliteDatabase {
    fn update_trade_overview(
        &mut self,
        trade: &Trade,
        funding: Decimal,
        capital_in_market: Decimal,
        capital_out_market: Decimal,
        taxed: Decimal,
        total_performance: Decimal,
    ) -> Result<TradeOverview, Box<dyn Error>> {
        WorkerTrade::update_trade_overview(
            &mut self.connection.lock().unwrap(),
            trade,
            funding,
            capital_in_market,
            capital_out_market,
            taxed,
            total_performance,
        )
    }
}

impl ReadOrderDB for SqliteDatabase {
    fn read_order(&mut self, id: Uuid) -> Result<Order, Box<dyn Error>> {
        WorkerOrder::read(&mut self.connection.lock().unwrap(), id)
    }
}
