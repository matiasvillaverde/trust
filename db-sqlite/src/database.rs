use crate::workers::{
    AccountBalanceDB, AccountDB, BrokerLogDB, WorkerOrder, WorkerRule, WorkerTrade,
    WorkerTradeGrade, WorkerTradingVehicle, WorkerTransaction,
};
use diesel::prelude::*;
use diesel::sql_query;
use model::DraftTrade;
use model::Status;
use model::{
    database::TradingVehicleUpsert,
    database::{AccountWrite, WriteAccountBalanceDB},
    Account, AccountBalanceRead, AccountBalanceWrite, AccountRead, Currency, DatabaseFactory,
    Order, OrderAction, OrderCategory, OrderRead, OrderWrite, ReadRuleDB, ReadTradeDB,
    ReadTradeGradeDB, ReadTradingVehicleDB, ReadTransactionDB, Rule, RuleName, Trade, TradeBalance,
    TradeGrade, TradingVehicle, TradingVehicleCategory, Transaction, TransactionCategory,
    WriteRuleDB, WriteTradeDB, WriteTradeGradeDB, WriteTradingVehicleDB, WriteTransactionDB,
};
use rust_decimal::Decimal;
use std::error::Error;
use std::sync::Arc;
use std::sync::Mutex;
use uuid::Uuid;

/// SQLite database implementation providing access to all database operations
pub struct SqliteDatabase {
    connection: Arc<Mutex<SqliteConnection>>,
}

impl std::fmt::Debug for SqliteDatabase {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SqliteDatabase")
            .field("connection", &"Arc<Mutex<SqliteConnection>>")
            .finish()
    }
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

    fn begin_savepoint(&mut self, name: &str) -> Result<(), Box<dyn Error>> {
        self.execute_savepoint_statement("SAVEPOINT", name)
    }

    fn release_savepoint(&mut self, name: &str) -> Result<(), Box<dyn Error>> {
        self.execute_savepoint_statement("RELEASE SAVEPOINT", name)
    }

    fn rollback_to_savepoint(&mut self, name: &str) -> Result<(), Box<dyn Error>> {
        self.execute_savepoint_statement("ROLLBACK TO SAVEPOINT", name)
    }

    fn account_balance_read(&self) -> Box<dyn AccountBalanceRead> {
        Box::new(AccountBalanceDB {
            connection: self.connection.clone(),
        })
    }

    fn account_balance_write(&self) -> Box<dyn AccountBalanceWrite> {
        Box::new(AccountBalanceDB {
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
    fn trade_balance_write(&self) -> Box<dyn WriteAccountBalanceDB> {
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

    fn trade_grade_read(&self) -> Box<dyn ReadTradeGradeDB> {
        Box::new(SqliteDatabase::new_from(self.connection.clone()))
    }

    fn trade_grade_write(&self) -> Box<dyn WriteTradeGradeDB> {
        Box::new(SqliteDatabase::new_from(self.connection.clone()))
    }
}

impl SqliteDatabase {
    fn validate_savepoint_name(name: &str) -> Result<(), Box<dyn Error>> {
        if name.is_empty() {
            return Err("savepoint name cannot be empty".into());
        }
        if !name.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'_') {
            return Err(format!(
                "invalid savepoint name '{name}': only ASCII alphanumeric and '_' are allowed"
            )
            .into());
        }
        Ok(())
    }

    fn execute_savepoint_statement(
        &mut self,
        statement: &str,
        savepoint: &str,
    ) -> Result<(), Box<dyn Error>> {
        Self::validate_savepoint_name(savepoint)?;
        let sql = format!("{statement} {savepoint}");
        let mut connection = self.connection.lock().unwrap_or_else(|e| {
            eprintln!("Failed to acquire connection lock: {e}");
            std::process::exit(1);
        });
        sql_query(sql).execute(&mut *connection)?;
        Ok(())
    }

    fn configure_connection(connection: &mut SqliteConnection) {
        sql_query(
            "CREATE INDEX IF NOT EXISTS idx_transactions_account_currency_category_active \
             ON transactions(account_id, currency, category, created_at) \
             WHERE deleted_at IS NULL",
        )
        .execute(connection)
        .unwrap_or_else(|e| {
            eprintln!(
                "Failed to create index idx_transactions_account_currency_category_active: {e}"
            );
            std::process::exit(1);
        });

        sql_query(
            "CREATE INDEX IF NOT EXISTS idx_transactions_trade_category_active \
             ON transactions(trade_id, category, created_at) \
             WHERE deleted_at IS NULL",
        )
        .execute(connection)
        .unwrap_or_else(|e| {
            eprintln!("Failed to create index idx_transactions_trade_category_active: {e}");
            std::process::exit(1);
        });

        sql_query(
            "CREATE INDEX IF NOT EXISTS idx_trades_account_status_currency_active \
             ON trades(account_id, status, currency) \
             WHERE deleted_at IS NULL",
        )
        .execute(connection)
        .unwrap_or_else(|e| {
            eprintln!("Failed to create index idx_trades_account_status_currency_active: {e}");
            std::process::exit(1);
        });

        sql_query(
            "CREATE INDEX IF NOT EXISTS idx_accounts_balances_account_currency_active \
             ON accounts_balances(account_id, currency) \
             WHERE deleted_at IS NULL",
        )
        .execute(connection)
        .unwrap_or_else(|e| {
            eprintln!("Failed to create index idx_accounts_balances_account_currency_active: {e}");
            std::process::exit(1);
        });
    }

    /// Create a new SQLite database connection from a URL
    pub fn new(url: &str) -> Self {
        let connection: SqliteConnection = Self::establish_connection(url);
        SqliteDatabase {
            connection: Arc::new(Mutex::new(connection)),
        }
    }

    /// Create a new SQLite database from an existing connection
    pub fn new_from(connection: Arc<Mutex<SqliteConnection>>) -> Self {
        SqliteDatabase { connection }
    }

    #[doc(hidden)]
    pub fn new_in_memory() -> Self {
        use diesel_migrations::*;
        pub const MIGRATIONS: EmbeddedMigrations = embed_migrations!();
        // This is only used for tests, so we use a simpler error handling approach
        let mut connection = SqliteConnection::establish(":memory:").unwrap_or_else(|e| {
            eprintln!("Failed to establish in-memory database connection: {e}");
            std::process::exit(1);
        });
        connection
            .run_pending_migrations(MIGRATIONS)
            .unwrap_or_else(|e| {
                eprintln!("Failed to run migrations on in-memory database: {e}");
                std::process::exit(1);
            });
        Self::configure_connection(&mut connection);
        connection.begin_test_transaction().unwrap_or_else(|e| {
            eprintln!("Failed to begin test transaction: {e}");
            std::process::exit(1);
        });
        SqliteDatabase {
            connection: Arc::new(Mutex::new(connection)),
        }
    }

    /// Establish a connection to the SQLite database.
    fn establish_connection(database_url: &str) -> SqliteConnection {
        let db_exists = std::path::Path::new(database_url).exists();
        // Use the database URL to establish a connection to the SQLite database
        let mut connection = SqliteConnection::establish(database_url).unwrap_or_else(|e| {
            eprintln!("Error connecting to {database_url}: {e}");
            std::process::exit(1);
        });

        // Run migrations only if it is a new DB
        if !db_exists {
            use diesel_migrations::*;
            pub const MIGRATIONS: EmbeddedMigrations = embed_migrations!();
            connection
                .run_pending_migrations(MIGRATIONS)
                .unwrap_or_else(|e| {
                    eprintln!("Failed to run migrations on new database: {e}");
                    std::process::exit(1);
                });
        }

        Self::configure_connection(&mut connection);
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
            &mut self.connection.lock().unwrap_or_else(|e| {
                eprintln!("Failed to acquire connection lock: {e}");
                std::process::exit(1);
            }),
            price,
            currency,
            quantity,
            action,
            category,
            trading_vehicle,
        )
    }

    fn update(&mut self, order: &Order) -> Result<Order, Box<dyn Error>> {
        WorkerOrder::update(
            &mut self.connection.lock().unwrap_or_else(|e| {
                eprintln!("Failed to acquire connection lock: {e}");
                std::process::exit(1);
            }),
            order,
        )
    }

    fn submit_of(&mut self, order: &Order, broker_order_id: Uuid) -> Result<Order, Box<dyn Error>> {
        WorkerOrder::update_submitted_at(
            &mut self.connection.lock().unwrap_or_else(|e| {
                eprintln!("Failed to acquire connection lock: {e}");
                std::process::exit(1);
            }),
            order,
            broker_order_id,
        )
    }

    fn filling_of(&mut self, order: &Order) -> Result<Order, Box<dyn Error>> {
        WorkerOrder::update_filled_at(
            &mut self.connection.lock().unwrap_or_else(|e| {
                eprintln!("Failed to acquire connection lock: {e}");
                std::process::exit(1);
            }),
            order,
        )
    }

    fn closing_of(&mut self, order: &Order) -> Result<Order, Box<dyn Error>> {
        WorkerOrder::update_closed_at(
            &mut self.connection.lock().unwrap_or_else(|e| {
                eprintln!("Failed to acquire connection lock: {e}");
                std::process::exit(1);
            }),
            order,
        )
    }
    fn update_price(
        &mut self,
        order: &Order,
        price: Decimal,
        new_broker_id: Uuid,
    ) -> Result<Order, Box<dyn Error>> {
        WorkerOrder::update_price(
            &mut self.connection.lock().unwrap_or_else(|e| {
                eprintln!("Failed to acquire connection lock: {e}");
                std::process::exit(1);
            }),
            order,
            price,
            new_broker_id,
        )
    }
}

impl WriteTransactionDB for SqliteDatabase {
    fn create_transaction_by_account_id(
        &mut self,
        account_id: Uuid,
        amount: rust_decimal::Decimal,
        currency: &Currency,
        category: TransactionCategory,
    ) -> Result<Transaction, Box<dyn Error>> {
        WorkerTransaction::create_transaction(
            &mut self.connection.lock().unwrap_or_else(|e| {
                eprintln!("Failed to acquire connection lock: {e}");
                std::process::exit(1);
            }),
            account_id,
            amount,
            currency,
            category,
        )
    }
}

impl ReadTradeGradeDB for SqliteDatabase {
    fn read_latest_for_trade(
        &mut self,
        trade_id: Uuid,
    ) -> Result<Option<TradeGrade>, Box<dyn Error>> {
        WorkerTradeGrade::read_latest_for_trade(
            &mut self.connection.lock().unwrap_or_else(|e| {
                eprintln!("Failed to acquire connection lock: {e}");
                std::process::exit(1);
            }),
            trade_id,
        )
    }

    fn read_for_account_days(
        &mut self,
        account_id: Uuid,
        days: u32,
    ) -> Result<Vec<TradeGrade>, Box<dyn Error>> {
        WorkerTradeGrade::read_for_account_days(
            &mut self.connection.lock().unwrap_or_else(|e| {
                eprintln!("Failed to acquire connection lock: {e}");
                std::process::exit(1);
            }),
            account_id,
            days,
        )
    }
}

impl WriteTradeGradeDB for SqliteDatabase {
    fn create_trade_grade(&mut self, grade: &TradeGrade) -> Result<TradeGrade, Box<dyn Error>> {
        WorkerTradeGrade::create(
            &mut self.connection.lock().unwrap_or_else(|e| {
                eprintln!("Failed to acquire connection lock: {e}");
                std::process::exit(1);
            }),
            grade,
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
            &mut self.connection.lock().unwrap_or_else(|e| {
                eprintln!("Failed to acquire connection lock: {e}");
                std::process::exit(1);
            }),
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
            &mut self.connection.lock().unwrap_or_else(|e| {
                eprintln!("Failed to acquire connection lock: {e}");
                std::process::exit(1);
            }),
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
            &mut self.connection.lock().unwrap_or_else(|e| {
                eprintln!("Failed to acquire connection lock: {e}");
                std::process::exit(1);
            }),
            account_id,
            currency,
        )
    }

    fn all_trade_transactions(
        &mut self,
        trade_id: Uuid,
    ) -> Result<Vec<Transaction>, Box<dyn Error>> {
        WorkerTransaction::read_all_trade_transactions(
            &mut self.connection.lock().unwrap_or_else(|e| {
                eprintln!("Failed to acquire connection lock: {e}");
                std::process::exit(1);
            }),
            trade_id,
        )
    }

    fn all_trade_funding_transactions(
        &mut self,
        trade_id: Uuid,
    ) -> Result<Vec<Transaction>, Box<dyn Error>> {
        WorkerTransaction::read_all_trade_transactions_for_category(
            &mut self.connection.lock().unwrap_or_else(|e| {
                eprintln!("Failed to acquire connection lock: {e}");
                std::process::exit(1);
            }),
            trade_id,
            TransactionCategory::FundTrade(trade_id),
        )
    }

    fn all_trade_taxes_transactions(
        &mut self,
        trade_id: Uuid,
    ) -> Result<Vec<Transaction>, Box<dyn Error>> {
        WorkerTransaction::read_all_trade_transactions_for_category(
            &mut self.connection.lock().unwrap_or_else(|e| {
                eprintln!("Failed to acquire connection lock: {e}");
                std::process::exit(1);
            }),
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
            &mut self.connection.lock().unwrap_or_else(|e| {
                eprintln!("Failed to acquire connection lock: {e}");
                std::process::exit(1);
            }),
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
            &mut self.connection.lock().unwrap_or_else(|e| {
                eprintln!("Failed to acquire connection lock: {e}");
                std::process::exit(1);
            }),
            account_id,
            currency,
        )
    }
}

impl ReadRuleDB for SqliteDatabase {
    fn read_all_rules(&mut self, account_id: Uuid) -> Result<Vec<Rule>, Box<dyn Error>> {
        WorkerRule::read_all(
            &mut self.connection.lock().unwrap_or_else(|e| {
                eprintln!("Failed to acquire connection lock: {e}");
                std::process::exit(1);
            }),
            account_id,
        )
    }

    fn rule_for_account(
        &mut self,
        account_id: Uuid,
        name: &RuleName,
    ) -> Result<Rule, Box<dyn Error>> {
        WorkerRule::read_for_account_with_name(
            &mut self.connection.lock().unwrap_or_else(|e| {
                eprintln!("Failed to acquire connection lock: {e}");
                std::process::exit(1);
            }),
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
            &mut self.connection.lock().unwrap_or_else(|e| {
                eprintln!("Failed to acquire connection lock: {e}");
                std::process::exit(1);
            }),
            name,
            description,
            priority,
            level,
            account,
        )
    }

    fn make_rule_inactive(&mut self, rule: &Rule) -> Result<Rule, Box<dyn Error>> {
        WorkerRule::make_inactive(
            &mut self.connection.lock().unwrap_or_else(|e| {
                eprintln!("Failed to acquire connection lock: {e}");
                std::process::exit(1);
            }),
            rule,
        )
    }
}

impl WriteTradingVehicleDB for SqliteDatabase {
    fn create_trading_vehicle(
        &mut self,
        symbol: &str,
        isin: Option<&str>,
        category: &TradingVehicleCategory,
        broker: &str,
    ) -> Result<TradingVehicle, Box<dyn Error>> {
        WorkerTradingVehicle::create(
            &mut self.connection.lock().unwrap_or_else(|e| {
                eprintln!("Failed to acquire connection lock: {e}");
                std::process::exit(1);
            }),
            symbol,
            isin,
            category,
            broker,
        )
    }

    fn upsert_trading_vehicle(
        &mut self,
        input: TradingVehicleUpsert,
    ) -> Result<TradingVehicle, Box<dyn Error>> {
        WorkerTradingVehicle::upsert(
            &mut self.connection.lock().unwrap_or_else(|e| {
                eprintln!("Failed to acquire connection lock: {e}");
                std::process::exit(1);
            }),
            input,
        )
    }
}

impl ReadTradingVehicleDB for SqliteDatabase {
    fn read_all_trading_vehicles(&mut self) -> Result<Vec<TradingVehicle>, Box<dyn Error>> {
        WorkerTradingVehicle::read_all(&mut self.connection.lock().unwrap_or_else(|e| {
            eprintln!("Failed to acquire connection lock: {e}");
            std::process::exit(1);
        }))
    }

    fn read_trading_vehicle(&mut self, id: Uuid) -> Result<TradingVehicle, Box<dyn Error>> {
        WorkerTradingVehicle::read(
            &mut self.connection.lock().unwrap_or_else(|e| {
                eprintln!("Failed to acquire connection lock: {e}");
                std::process::exit(1);
            }),
            id,
        )
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
            &mut self.connection.lock().unwrap_or_else(|e| {
                eprintln!("Failed to acquire connection lock: {e}");
                std::process::exit(1);
            }),
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
        WorkerTrade::update_trade_status(
            &mut self.connection.lock().unwrap_or_else(|e| {
                eprintln!("Failed to acquire connection lock: {e}");
                std::process::exit(1);
            }),
            status,
            trade,
        )
    }
}

impl ReadTradeDB for SqliteDatabase {
    fn read_trade(&mut self, id: Uuid) -> Result<Trade, Box<dyn Error>> {
        WorkerTrade::read_trade(
            &mut self.connection.lock().unwrap_or_else(|e| {
                eprintln!("Failed to acquire connection lock: {e}");
                std::process::exit(1);
            }),
            id,
        )
    }

    fn read_trade_balance(&mut self, balance_id: Uuid) -> Result<TradeBalance, Box<dyn Error>> {
        WorkerTrade::read_balance(
            &mut self.connection.lock().unwrap_or_else(|e| {
                eprintln!("Failed to acquire connection lock: {e}");
                std::process::exit(1);
            }),
            balance_id,
        )
    }

    fn all_open_trades_for_currency(
        &mut self,
        account_id: Uuid,
        currency: &Currency,
    ) -> Result<Vec<Trade>, Box<dyn Error>> {
        WorkerTrade::read_all_funded_trades_for_currency(
            &mut self.connection.lock().unwrap_or_else(|e| {
                eprintln!("Failed to acquire connection lock: {e}");
                std::process::exit(1);
            }),
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
            &mut self.connection.lock().unwrap_or_else(|e| {
                eprintln!("Failed to acquire connection lock: {e}");
                std::process::exit(1);
            }),
            account_id,
            status,
        )
    }
}

impl WriteAccountBalanceDB for SqliteDatabase {
    fn update_trade_balance(
        &mut self,
        trade: &Trade,
        funding: Decimal,
        capital_in_market: Decimal,
        capital_out_market: Decimal,
        taxed: Decimal,
        total_performance: Decimal,
    ) -> Result<TradeBalance, Box<dyn Error>> {
        WorkerTrade::update_trade_balance(
            &mut self.connection.lock().unwrap_or_else(|e| {
                eprintln!("Failed to acquire connection lock: {e}");
                std::process::exit(1);
            }),
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
        WorkerOrder::read(
            &mut self.connection.lock().unwrap_or_else(|e| {
                eprintln!("Failed to acquire connection lock: {e}");
                std::process::exit(1);
            }),
            id,
        )
    }
}
