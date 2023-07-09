use crate::workers::{
    AccountDB, AccountOverviewDB, BrokerLogDB, WorkerOrder, WorkerRule, WorkerTrade,
    WorkerTradingVehicle, WorkerTransaction,
};
use diesel::prelude::*;
use model::DraftTrade;
use model::Status;
use model::{
    database::{AccountWrite, WriteTradeOverviewDB},
    Account, Currency, DatabaseFactory, Order, OrderAction, OrderCategory, AccountRead,
    AccountOverviewRead, OrderRead, ReadRuleDB, ReadTradeDB, ReadTradingVehicleDB,
    ReadTransactionDB, Rule, RuleName, Trade, TradeOverview, TradingVehicle,
    TradingVehicleCategory, Transaction, TransactionCategory, AccountOverviewWrite, OrderWrite,
    WriteRuleDB, WriteTradeDB, WriteTradingVehicleDB, WriteTransactionDB,
};
use rust_decimal::Decimal;
use std::error::Error;
use std::sync::Arc;
use std::sync::Mutex;
use uuid::Uuid;

pub struct SqliteDatabase {
    connection: Arc<Mutex<SqliteConnection>>,
}

impl DatabaseFactory for SqliteDatabase {
    fn account_read(&self) -> Box<dyn AccountRead> {
        Box::new(AccountDB {
            connection: self.connection.clone(),
        })
    }

    fn account_write(&self) -> Box<dyn AccountWrite> {
        Box::new(AccountDB {
            connection: self.connection.clone(),
        })
    }

    fn log_read(&self) -> Box<dyn model::ReadBrokerLogsDB> {
        Box::new(BrokerLogDB {
            connection: self.connection.clone(),
        })
    }

    fn log_write(&self) -> Box<dyn model::WriteBrokerLogsDB> {
        Box::new(BrokerLogDB {
            connection: self.connection.clone(),
        })
    }

    fn account_overview_read(&self) -> Box<dyn AccountOverviewRead> {
        Box::new(AccountOverviewDB {
            connection: self.connection.clone(),
        })
    }

    fn account_overview_write(&self) -> Box<dyn AccountOverviewWrite> {
        Box::new(AccountOverviewDB {
            connection: self.connection.clone(),
        })
    }

    fn order_read(&self) -> Box<dyn OrderRead> {
        Box::new(SqliteDatabase::new_from(self.connection.clone()))
    }
    fn order_write(&self) -> Box<dyn OrderWrite> {
        Box::new(SqliteDatabase::new_from(self.connection.clone()))
    }

    fn transaction_read(&self) -> Box<dyn ReadTransactionDB> {
        Box::new(SqliteDatabase::new_from(self.connection.clone()))
    }
    fn transaction_write(&self) -> Box<dyn WriteTransactionDB> {
        Box::new(SqliteDatabase::new_from(self.connection.clone()))
    }
    fn trade_read(&self) -> Box<dyn ReadTradeDB> {
        Box::new(SqliteDatabase::new_from(self.connection.clone()))
    }
    fn trade_write(&self) -> Box<dyn WriteTradeDB> {
        Box::new(SqliteDatabase::new_from(self.connection.clone()))
    }
    fn trade_overview_write(&self) -> Box<dyn WriteTradeOverviewDB> {
        Box::new(SqliteDatabase::new_from(self.connection.clone()))
    }
    fn rule_read(&self) -> Box<dyn ReadRuleDB> {
        Box::new(SqliteDatabase::new_from(self.connection.clone()))
    }
    fn rule_write(&self) -> Box<dyn WriteRuleDB> {
        Box::new(SqliteDatabase::new_from(self.connection.clone()))
    }
    fn trading_vehicle_read(&self) -> Box<dyn ReadTradingVehicleDB> {
        Box::new(SqliteDatabase::new_from(self.connection.clone()))
    }
    fn trading_vehicle_write(&self) -> Box<dyn WriteTradingVehicleDB> {
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

impl OrderWrite for SqliteDatabase {
    fn create(
        &mut self,
        trading_vehicle: &TradingVehicle,
        quantity: i64,
        price: Decimal,
        currency: &Currency,
        action: &OrderAction,
        category: &OrderCategory,
    ) -> Result<Order, Box<dyn Error>> {
        WorkerOrder::create(
            &mut self.connection.lock().unwrap(),
            price,
            currency,
            quantity,
            action,
            category,
            trading_vehicle,
        )
    }

    fn update(&mut self, order: &Order) -> Result<Order, Box<dyn Error>> {
        WorkerOrder::update(&mut self.connection.lock().unwrap(), order)
    }

    fn submit_of(
        &mut self,
        order: &Order,
        broker_order_id: Uuid,
    ) -> Result<Order, Box<dyn Error>> {
        WorkerOrder::update_submitted_at(
            &mut self.connection.lock().unwrap(),
            order,
            broker_order_id,
        )
    }

    fn filling_of(&mut self, order: &Order) -> Result<Order, Box<dyn Error>> {
        WorkerOrder::update_filled_at(&mut self.connection.lock().unwrap(), order)
    }

    fn closing_of(&mut self, order: &Order) -> Result<Order, Box<dyn Error>> {
        WorkerOrder::update_closed_at(&mut self.connection.lock().unwrap(), order)
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

    fn all_account_transactions_funding_in_submitted_trades(
        &mut self,
        account_id: Uuid,
        currency: &Currency,
    ) -> Result<Vec<Transaction>, Box<dyn Error>> {
        WorkerTransaction::all_account_transactions_in_trade(
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
        trade_id: Uuid,
    ) -> Result<Vec<Transaction>, Box<dyn Error>> {
        WorkerTransaction::read_all_trade_transactions(
            &mut self.connection.lock().unwrap(),
            trade_id,
        )
    }

    fn all_trade_funding_transactions(
        &mut self,
        trade_id: Uuid,
    ) -> Result<Vec<Transaction>, Box<dyn Error>> {
        WorkerTransaction::read_all_trade_transactions_for_category(
            &mut self.connection.lock().unwrap(),
            trade_id,
            TransactionCategory::FundTrade(trade_id),
        )
    }

    fn all_trade_taxes_transactions(
        &mut self,
        trade_id: Uuid,
    ) -> Result<Vec<Transaction>, Box<dyn Error>> {
        WorkerTransaction::read_all_trade_transactions_for_category(
            &mut self.connection.lock().unwrap(),
            trade_id,
            TransactionCategory::PaymentTax(trade_id),
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
        name: &model::RuleName,
        description: &str,
        priority: u32,
        level: &model::RuleLevel,
    ) -> Result<model::Rule, Box<dyn Error>> {
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
        draft: DraftTrade,
        stop: &Order,
        entry: &Order,
        target: &Order,
    ) -> Result<Trade, Box<dyn Error>> {
        WorkerTrade::create(
            &mut self.connection.lock().unwrap(),
            draft,
            stop,
            entry,
            target,
        )
    }

    fn update_trade_status(
        &mut self,
        status: Status,
        trade: &Trade,
    ) -> Result<Trade, Box<dyn Error>> {
        WorkerTrade::update_trade_status(&mut self.connection.lock().unwrap(), status, trade)
    }
}

impl ReadTradeDB for SqliteDatabase {
    fn read_trade(&mut self, id: Uuid) -> Result<Trade, Box<dyn Error>> {
        WorkerTrade::read_trade(&mut self.connection.lock().unwrap(), id)
    }

    fn all_open_trades_for_currency(
        &mut self,
        account_id: Uuid,
        currency: &Currency,
    ) -> Result<Vec<Trade>, Box<dyn Error>> {
        WorkerTrade::read_all_funded_trades_for_currency(
            &mut self.connection.lock().unwrap(),
            account_id,
            currency,
        )
    }

    fn read_trades_with_status(
        &mut self,
        account_id: Uuid,
        status: Status,
    ) -> Result<Vec<Trade>, Box<dyn Error>> {
        WorkerTrade::read_all_trades_with_status(
            &mut self.connection.lock().unwrap(),
            account_id,
            status,
        )
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

impl OrderRead for SqliteDatabase {
    fn for_id(&mut self, id: Uuid) -> Result<Order, Box<dyn Error>> {
        WorkerOrder::read(&mut self.connection.lock().unwrap(), id)
    }
}
