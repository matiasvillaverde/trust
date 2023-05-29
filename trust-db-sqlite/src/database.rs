use crate::workers::{
    WorkerAccount, WorkerAccountOverview, WorkerOrder, WorkerPrice, WorkerRule, WorkerTarget,
    WorkerTrade, WorkerTradingVehicle, WorkerTransaction,
};
use diesel::prelude::*;
use rust_decimal::Decimal;
use std::error::Error;
use trust_model::{
    Account, AccountOverview, Currency, Database, Order, OrderAction, OrderCategory, Price,
    ReadAccountDB, Rule, RuleName, Target, Trade, TradeCategory, TradeOverview, TradingVehicle,
    TradingVehicleCategory, Transaction, TransactionCategory, WriteOrderDB,
};
use uuid::Uuid;

/// SqliteDatabase is a struct that contains methods for interacting with the
/// SQLite database.
pub struct SqliteDatabase {
    /// The connection to the SQLite database.
    connection: SqliteConnection,
}

impl SqliteDatabase {
    pub fn new(url: &str) -> Self {
        let connection: SqliteConnection = Self::establish_connection(url);
        SqliteDatabase { connection }
    }

    #[doc(hidden)]
    pub fn new_in_memory() -> Self {
        use diesel_migrations::*;
        pub const MIGRATIONS: EmbeddedMigrations = embed_migrations!();
        let mut connection = SqliteConnection::establish(":memory:").unwrap();
        connection.run_pending_migrations(MIGRATIONS).unwrap();
        connection.begin_test_transaction().unwrap();
        SqliteDatabase { connection }
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

impl ReadAccountDB for SqliteDatabase {
    fn read_account(&mut self, name: &str) -> Result<Account, Box<dyn Error>> {
        return WorkerAccount::read_account(&mut self.connection, name);
    }

    fn read_account_id(&mut self, id: Uuid) -> Result<Account, Box<dyn Error>> {
        WorkerAccount::read(&mut self.connection, id)
    }

    fn read_all_accounts(&mut self) -> Result<Vec<Account>, Box<dyn Error>> {
        let accounts = WorkerAccount::read_all_accounts(&mut self.connection);
        accounts
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
            &mut self.connection,
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
        WorkerTarget::create(&mut self.connection, price, currency, order, trade)
    }
}

impl Database for SqliteDatabase {
    fn new_account(&mut self, name: &str, description: &str) -> Result<Account, Box<dyn Error>> {
        let account = WorkerAccount::create_account(&mut self.connection, name, description);
        account
    }

    fn read_account_overview(
        &mut self,
        account_id: Uuid,
    ) -> Result<Vec<AccountOverview>, Box<dyn Error>> {
        WorkerAccountOverview::read(&mut self.connection, account_id)
    }

    fn read_account_overview_currency(
        &mut self,
        account_id: Uuid,
        currency: &Currency,
    ) -> Result<AccountOverview, Box<dyn Error>> {
        WorkerAccountOverview::read_for_currency(&mut self.connection, account_id, currency)
    }

    fn new_account_overview(
        &mut self,
        account: &Account,
        currency: &Currency,
    ) -> Result<AccountOverview, Box<dyn Error>> {
        let overview = WorkerAccountOverview::create(&mut self.connection, account, currency)?;
        Ok(overview)
    }

    fn update_account_overview(
        &mut self,
        account: &Account,
        currency: &Currency,
        total_balance: rust_decimal::Decimal,
        total_available: rust_decimal::Decimal,
    ) -> Result<AccountOverview, Box<dyn Error>> {
        let overview =
            WorkerAccountOverview::read_for_currency(&mut self.connection, account.id, currency)?;
        let updated_overview = WorkerAccountOverview::update_total_available(
            &mut self.connection,
            overview,
            total_available,
        )?;
        let updated_overview = WorkerAccountOverview::update_total_balance(
            &mut self.connection,
            updated_overview,
            total_balance,
        )?;
        Ok(updated_overview)
    }

    fn update_account_overview_trade(
        &mut self,
        account: &Account,
        currency: &Currency,
        total_available: Decimal,
        total_in_trade: Decimal,
    ) -> Result<AccountOverview, Box<dyn Error>> {
        let overview =
            WorkerAccountOverview::read_for_currency(&mut self.connection, account.id, currency)?;
        let updated_overview = WorkerAccountOverview::update_total_available(
            &mut self.connection,
            overview,
            total_available,
        )?;

        let updated_total_in_trade = WorkerAccountOverview::update_total_in_trade(
            &mut self.connection,
            updated_overview,
            total_in_trade,
        )?;

        Ok(updated_total_in_trade)
    }

    fn new_price(
        &mut self,
        currency: &Currency,
        amount: rust_decimal::Decimal,
    ) -> Result<Price, Box<dyn Error>> {
        WorkerPrice::create(&mut self.connection, currency, amount)
    }

    fn read_price(&mut self, id: uuid::Uuid) -> Result<Price, Box<dyn Error>> {
        WorkerPrice::read(&mut self.connection, id)
    }

    fn new_transaction(
        &mut self,
        account: &Account,
        amount: rust_decimal::Decimal,
        currency: &Currency,
        category: TransactionCategory,
    ) -> Result<Transaction, Box<dyn Error>> {
        WorkerTransaction::create_transaction(
            &mut self.connection,
            account.id,
            amount,
            currency,
            category,
        )
    }

    fn all_trade_transactions_excluding_taxes(
        &mut self,
        account_id: Uuid,
        currency: &Currency,
    ) -> Result<Vec<Transaction>, Box<dyn Error>> {
        WorkerTransaction::read_all_trade_transactions_excluding_taxes(
            &mut self.connection,
            account_id,
            currency,
        )
    }

    fn all_open_trades_for_currency(
        &mut self,
        account_id: Uuid,
        currency: &Currency,
    ) -> Result<Vec<Trade>, Box<dyn Error>> {
        WorkerTrade::read_all_open_trades_for_currency(&mut self.connection, account_id, currency)
    }

    fn all_open_trades(&mut self, account_id: Uuid) -> Result<Vec<Trade>, Box<dyn Error>> {
        WorkerTrade::read_all_open_trades(&mut self.connection, account_id)
    }

    fn all_trades_in_market(&mut self, account_id: Uuid) -> Result<Vec<Trade>, Box<dyn Error>> {
        WorkerTrade::read_all_trades_in_market(&mut self.connection, account_id)
    }

    fn all_transaction_excluding_current_month_and_taxes(
        &mut self,
        account_id: Uuid,
        currency: &Currency,
    ) -> Result<Vec<Transaction>, Box<dyn Error>> {
        WorkerTransaction::read_all_transaction_excluding_current_month_and_taxes(
            &mut self.connection,
            account_id,
            currency,
        )
    }

    fn create_rule(
        &mut self,
        account: &Account,
        name: &trust_model::RuleName,
        description: &str,
        priority: u32,
        level: &trust_model::RuleLevel,
    ) -> Result<trust_model::Rule, Box<dyn Error>> {
        WorkerRule::create(
            &mut self.connection,
            name,
            description,
            priority,
            level,
            account,
        )
    }

    fn read_all_rules(&mut self, account_id: Uuid) -> Result<Vec<Rule>, Box<dyn Error>> {
        WorkerRule::read_all(&mut self.connection, account_id)
    }

    fn make_rule_inactive(&mut self, rule: &Rule) -> Result<Rule, Box<dyn Error>> {
        WorkerRule::make_inactive(&mut self.connection, rule)
    }

    fn rule_for_account(
        &mut self,
        account_id: Uuid,
        name: &RuleName,
    ) -> Result<Rule, Box<dyn Error>> {
        WorkerRule::read_for_account_with_name(&mut self.connection, account_id, name)
    }

    fn create_trading_vehicle(
        &mut self,
        symbol: &str,
        isin: &str,
        category: &TradingVehicleCategory,
        broker: &str,
    ) -> Result<TradingVehicle, Box<dyn Error>> {
        WorkerTradingVehicle::create(&mut self.connection, symbol, isin, category, broker)
    }

    fn read_all_trading_vehicles(&mut self) -> Result<Vec<TradingVehicle>, Box<dyn Error>> {
        WorkerTradingVehicle::read_all(&mut self.connection)
    }

    fn read_trading_vehicle(&mut self, id: Uuid) -> Result<TradingVehicle, Box<dyn Error>> {
        WorkerTradingVehicle::read(&mut self.connection, id)
    }

    fn record_order_execution(&mut self, order: &Order) -> Result<Order, Box<dyn Error>> {
        WorkerOrder::record_execution(&mut self.connection, order)
    }

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
            &mut self.connection,
            category,
            currency,
            trading_vehicle,
            safety_stop,
            entry,
            account,
        )
    }

    fn read_trade(&mut self, id: Uuid) -> Result<Trade, Box<dyn Error>> {
        WorkerTrade::read_trade(&mut self.connection, id)
    }

    fn read_all_new_trades(&mut self, account_id: Uuid) -> Result<Vec<Trade>, Box<dyn Error>> {
        WorkerTrade::read_all_new_trades(&mut self.connection, account_id)
    }

    fn approve_trade(&mut self, trade: &Trade) -> Result<Trade, Box<dyn Error>> {
        WorkerTrade::approve_trade(&mut self.connection, trade)
    }

    fn update_trade_overview(
        &mut self,
        trade: &Trade,
        total_input: Decimal,
    ) -> Result<TradeOverview, Box<dyn Error>> {
        WorkerTrade::update_trade_input(&mut self.connection, trade, total_input)
    }

    fn update_trade_overview_in(
        &mut self,
        trade: &Trade,
        total_in_market: Decimal,
    ) -> Result<TradeOverview, Box<dyn Error>> {
        WorkerTrade::update_trade_overview_in(&mut self.connection, trade, total_in_market)
    }

    fn update_trade_overview_out(
        &mut self,
        trade: &Trade,
        total_out_market: Decimal,
        total_taxable: Decimal,
        total_performance: Decimal,
    ) -> Result<TradeOverview, Box<dyn Error>> {
        WorkerTrade::update_trade_overview_out(
            &mut self.connection,
            trade,
            total_out_market,
            total_taxable,
            total_performance,
        )
    }

    fn update_trade_executed_at(&mut self, trade: &Trade) -> Result<Trade, Box<dyn Error>> {
        WorkerTrade::update_executed_at(&mut self.connection, trade)
    }
}
