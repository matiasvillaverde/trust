use crate::workers::{
    AccountBalanceDB, AccountDB, AdvisoryDB, BrokerLogDB, DistributionDB, WorkerExecution,
    WorkerLevel, WorkerMistake, WorkerOrder, WorkerRule, WorkerSessionPlan, WorkerTrade,
    WorkerTradeEvent, WorkerTradeGrade, WorkerTradingVehicle, WorkerTransaction,
};
use crate::{backup, backup::ImportOptions};
use diesel::prelude::*;
use diesel::sql_query;
use model::DraftTrade;
use model::Status;
use model::{
    database::TradingVehicleUpsert,
    database::{AccountWrite, WriteAccountBalanceDB},
    Account, AccountBalanceRead, AccountBalanceWrite, AccountRead, Currency, DatabaseFactory,
    DistributionRead, DistributionWrite, Execution, Level, LevelAdjustmentRules, LevelChange,
    Mistake, Order, OrderAction, OrderCategory, OrderRead, OrderWrite, ReadExecutionDB,
    ReadLevelDB, ReadMistakeDB, ReadRuleDB, ReadSessionPlanDB, ReadTradeDB, ReadTradeEventDB,
    ReadTradeGradeDB, ReadTradingVehicleDB, ReadTransactionDB, Rule, RuleName, SessionPlan,
    SessionPlanClose, Trade, TradeBalance, TradeEvent, TradeGrade, TradingVehicle,
    TradingVehicleCategory, Transaction, TransactionCategory, WriteExecutionDB, WriteLevelDB,
    WriteMistakeDB, WriteRuleDB, WriteSessionPlanDB, WriteTradeDB, WriteTradeEventDB,
    WriteTradeGradeDB, WriteTradingVehicleDB, WriteTransactionDB,
};
use rust_decimal::Decimal;
use std::error::Error;
use std::path::Path;
use std::sync::{Arc, Mutex, MutexGuard};
use uuid::Uuid;

/// SQLite database implementation providing access to all database operations
pub struct SqliteDatabase {
    connection: Arc<Mutex<SqliteConnection>>,
}

fn fatal_database_error(context: &str, error: impl std::fmt::Display) -> ! {
    eprintln!("{context}: {error}");
    std::process::exit(1);
}

fn lock_connection_or_exit(
    connection: &Arc<Mutex<SqliteConnection>>,
) -> MutexGuard<'_, SqliteConnection> {
    match connection.lock() {
        Ok(connection) => connection,
        Err(error) => fatal_database_error("Failed to acquire connection lock", error),
    }
}

fn execute_sql_or_exit(connection: &mut SqliteConnection, sql: &str, context: &str) {
    if let Err(error) = sql_query(sql).execute(connection) {
        fatal_database_error(context, error);
    }
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

    fn execution_read(&self) -> Box<dyn ReadExecutionDB> {
        Box::new(SqliteDatabase::new_from(self.connection.clone()))
    }

    fn execution_write(&self) -> Box<dyn WriteExecutionDB> {
        Box::new(SqliteDatabase::new_from(self.connection.clone()))
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

    fn mistake_read(&self) -> Box<dyn ReadMistakeDB> {
        Box::new(SqliteDatabase::new_from(self.connection.clone()))
    }

    fn mistake_write(&self) -> Box<dyn WriteMistakeDB> {
        Box::new(SqliteDatabase::new_from(self.connection.clone()))
    }

    fn session_plan_read(&self) -> Box<dyn ReadSessionPlanDB> {
        Box::new(SqliteDatabase::new_from(self.connection.clone()))
    }

    fn session_plan_write(&self) -> Box<dyn WriteSessionPlanDB> {
        Box::new(SqliteDatabase::new_from(self.connection.clone()))
    }

    fn trade_event_read(&self) -> Box<dyn ReadTradeEventDB> {
        Box::new(SqliteDatabase::new_from(self.connection.clone()))
    }

    fn trade_event_write(&self) -> Box<dyn WriteTradeEventDB> {
        Box::new(SqliteDatabase::new_from(self.connection.clone()))
    }

    fn trade_grade_read(&self) -> Box<dyn ReadTradeGradeDB> {
        Box::new(SqliteDatabase::new_from(self.connection.clone()))
    }

    fn trade_grade_write(&self) -> Box<dyn WriteTradeGradeDB> {
        Box::new(SqliteDatabase::new_from(self.connection.clone()))
    }

    fn level_read(&self) -> Box<dyn ReadLevelDB> {
        Box::new(SqliteDatabase::new_from(self.connection.clone()))
    }

    fn level_write(&self) -> Box<dyn WriteLevelDB> {
        Box::new(SqliteDatabase::new_from(self.connection.clone()))
    }

    fn distribution_read(&self) -> Box<dyn DistributionRead> {
        Box::new(DistributionDB {
            connection: self.connection.clone(),
        })
    }

    fn distribution_write(&self) -> Box<dyn DistributionWrite> {
        Box::new(DistributionDB {
            connection: self.connection.clone(),
        })
    }

    fn advisory_read(&self) -> Box<dyn model::AdvisoryRead> {
        Box::new(AdvisoryDB {
            connection: self.connection.clone(),
        })
    }

    fn advisory_write(&self) -> Box<dyn model::AdvisoryWrite> {
        Box::new(AdvisoryDB {
            connection: self.connection.clone(),
        })
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
        let mut connection = lock_connection_or_exit(&self.connection);
        sql_query(sql).execute(&mut *connection)?;
        Ok(())
    }

    fn configure_connection(connection: &mut SqliteConnection) {
        // Enforce relational integrity. SQLite does not enable FK constraints by default.
        execute_sql_or_exit(
            connection,
            "PRAGMA foreign_keys = ON;",
            "Failed to enable foreign_keys pragma",
        );

        execute_sql_or_exit(
            connection,
            "CREATE INDEX IF NOT EXISTS idx_transactions_account_currency_category_active \
             ON transactions(account_id, currency, category, created_at) \
             WHERE deleted_at IS NULL",
            "Failed to create index idx_transactions_account_currency_category_active",
        );

        execute_sql_or_exit(
            connection,
            "CREATE INDEX IF NOT EXISTS idx_transactions_trade_category_active \
             ON transactions(trade_id, category, created_at) \
             WHERE deleted_at IS NULL",
            "Failed to create index idx_transactions_trade_category_active",
        );

        execute_sql_or_exit(
            connection,
            "CREATE INDEX IF NOT EXISTS idx_trades_account_status_currency_active \
             ON trades(account_id, status, currency) \
             WHERE deleted_at IS NULL",
            "Failed to create index idx_trades_account_status_currency_active",
        );

        execute_sql_or_exit(
            connection,
            "CREATE INDEX IF NOT EXISTS idx_accounts_balances_account_currency_active \
             ON accounts_balances(account_id, currency) \
             WHERE deleted_at IS NULL",
            "Failed to create index idx_accounts_balances_account_currency_active",
        );
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
        let mut connection = match SqliteConnection::establish(":memory:") {
            Ok(connection) => connection,
            Err(error) => {
                fatal_database_error("Failed to establish in-memory database connection", error)
            }
        };
        if let Err(error) = connection.run_pending_migrations(MIGRATIONS) {
            fatal_database_error("Failed to run migrations on in-memory database", error);
        }
        Self::configure_connection(&mut connection);
        if let Err(error) = connection.begin_test_transaction() {
            fatal_database_error("Failed to begin test transaction", error);
        }
        SqliteDatabase {
            connection: Arc::new(Mutex::new(connection)),
        }
    }

    /// Establish a connection to the SQLite database.
    fn establish_connection(database_url: &str) -> SqliteConnection {
        let db_exists = std::path::Path::new(database_url).exists();
        // Use the database URL to establish a connection to the SQLite database
        let mut connection = match SqliteConnection::establish(database_url) {
            Ok(connection) => connection,
            Err(error) => {
                fatal_database_error(&format!("Error connecting to {database_url}"), error)
            }
        };

        // Run migrations only if it is a new DB
        if !db_exists {
            use diesel_migrations::*;
            pub const MIGRATIONS: EmbeddedMigrations = embed_migrations!();
            if let Err(error) = connection.run_pending_migrations(MIGRATIONS) {
                fatal_database_error("Failed to run migrations on new database", error);
            }
        }

        Self::configure_connection(&mut connection);
        connection
    }

    /// Export a full JSON backup of the DB to `path`.
    pub fn export_backup_to_path(&self, path: &Path) -> Result<(), Box<dyn Error>> {
        let mut connection = lock_connection_or_exit(&self.connection);
        backup::export_to_path(&mut connection, path).map_err(|e| Box::new(e) as Box<dyn Error>)
    }

    /// Import a full JSON backup from `path`.
    ///
    /// This operation is atomic. See `backup::ImportMode` for behavior.
    pub fn import_backup_from_path(
        &mut self,
        path: &Path,
        options: ImportOptions,
    ) -> Result<backup::ImportReport, Box<dyn Error>> {
        let mut connection = lock_connection_or_exit(&self.connection);
        let backup =
            backup::read_backup_from_path(path).map_err(|e| Box::new(e) as Box<dyn Error>)?;
        backup::import_backup(&mut connection, &backup, options)
            .map_err(|e| Box::new(e) as Box<dyn Error>)
    }
}

impl OrderWrite for SqliteDatabase {
    fn create(
        &mut self,
        trading_vehicle: &TradingVehicle,
        quantity: Decimal,
        price: Decimal,
        currency: &Currency,
        action: &OrderAction,
        category: &OrderCategory,
    ) -> Result<Order, Box<dyn Error>> {
        WorkerOrder::create(
            &mut lock_connection_or_exit(&self.connection),
            price,
            currency,
            quantity,
            action,
            category,
            trading_vehicle,
        )
    }

    fn update(&mut self, order: &Order) -> Result<Order, Box<dyn Error>> {
        WorkerOrder::update(&mut lock_connection_or_exit(&self.connection), order)
    }

    fn submit_of(
        &mut self,
        order: &Order,
        broker_order_id: String,
    ) -> Result<Order, Box<dyn Error>> {
        WorkerOrder::update_submitted_at(
            &mut lock_connection_or_exit(&self.connection),
            order,
            broker_order_id,
        )
    }

    fn filling_of(&mut self, order: &Order) -> Result<Order, Box<dyn Error>> {
        WorkerOrder::update_filled_at(&mut lock_connection_or_exit(&self.connection), order)
    }

    fn closing_of(&mut self, order: &Order) -> Result<Order, Box<dyn Error>> {
        WorkerOrder::update_closed_at(&mut lock_connection_or_exit(&self.connection), order)
    }
    fn update_price(
        &mut self,
        order: &Order,
        price: Decimal,
        new_broker_id: String,
    ) -> Result<Order, Box<dyn Error>> {
        WorkerOrder::update_price(
            &mut lock_connection_or_exit(&self.connection),
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
            &mut lock_connection_or_exit(&self.connection),
            account_id,
            amount,
            currency,
            category,
        )
    }

    fn create_transfer_pair(
        &mut self,
        from_account: &Account,
        to_account: &Account,
        amount: Decimal,
        currency: &Currency,
        withdrawal_category: TransactionCategory,
        deposit_category: TransactionCategory,
    ) -> Result<(Transaction, Transaction), Box<dyn Error>> {
        let withdrawal_amount = Decimal::ZERO
            .checked_sub(amount)
            .ok_or("Invalid withdrawal amount")?;
        let connection = &mut lock_connection_or_exit(&self.connection);

        connection.transaction::<(Transaction, Transaction), Box<dyn Error>, _>(|conn| {
            let withdrawal_tx = WorkerTransaction::create_transaction(
                conn,
                from_account.id,
                withdrawal_amount,
                currency,
                withdrawal_category,
            )?;
            let deposit_tx = WorkerTransaction::create_transaction(
                conn,
                to_account.id,
                amount,
                currency,
                deposit_category,
            )?;

            Ok((withdrawal_tx, deposit_tx))
        })
    }
}

impl ReadMistakeDB for SqliteDatabase {
    fn read_mistakes_for_trade(&mut self, trade_id: Uuid) -> Result<Vec<Mistake>, Box<dyn Error>> {
        WorkerMistake::read_for_trade(&mut lock_connection_or_exit(&self.connection), trade_id)
    }

    fn read_mistakes_for_account_in_period(
        &mut self,
        account_id: Uuid,
        start_at: chrono::NaiveDateTime,
        end_at: chrono::NaiveDateTime,
    ) -> Result<Vec<Mistake>, Box<dyn Error>> {
        WorkerMistake::read_for_account_in_period(
            &mut lock_connection_or_exit(&self.connection),
            account_id,
            start_at,
            end_at,
        )
    }
}

impl WriteMistakeDB for SqliteDatabase {
    fn create_mistake(&mut self, mistake: &Mistake) -> Result<Mistake, Box<dyn Error>> {
        WorkerMistake::create(&mut lock_connection_or_exit(&self.connection), mistake)
    }
}

impl ReadSessionPlanDB for SqliteDatabase {
    fn read_open_session(
        &mut self,
        account_id: Uuid,
    ) -> Result<Option<SessionPlan>, Box<dyn Error>> {
        WorkerSessionPlan::read_open(&mut lock_connection_or_exit(&self.connection), account_id)
    }

    fn read_session_plans_for_account(
        &mut self,
        account_id: Uuid,
        start_at: chrono::NaiveDateTime,
        end_at: chrono::NaiveDateTime,
    ) -> Result<Vec<SessionPlan>, Box<dyn Error>> {
        WorkerSessionPlan::read_for_account_in_period(
            &mut lock_connection_or_exit(&self.connection),
            account_id,
            start_at,
            end_at,
        )
    }
}

impl WriteSessionPlanDB for SqliteDatabase {
    fn create_session_plan(
        &mut self,
        session_plan: &SessionPlan,
    ) -> Result<SessionPlan, Box<dyn Error>> {
        WorkerSessionPlan::create(&mut lock_connection_or_exit(&self.connection), session_plan)
    }

    fn close_session_plan(
        &mut self,
        close: &SessionPlanClose,
    ) -> Result<SessionPlan, Box<dyn Error>> {
        WorkerSessionPlan::close(&mut lock_connection_or_exit(&self.connection), close)
    }
}

impl ReadTradeEventDB for SqliteDatabase {
    fn read_trade_events_for_trade(
        &mut self,
        trade_id: Uuid,
    ) -> Result<Vec<TradeEvent>, Box<dyn Error>> {
        WorkerTradeEvent::read_for_trade(&mut lock_connection_or_exit(&self.connection), trade_id)
    }
}

impl WriteTradeEventDB for SqliteDatabase {
    fn create_trade_event(&mut self, event: &TradeEvent) -> Result<TradeEvent, Box<dyn Error>> {
        WorkerTradeEvent::create(&mut lock_connection_or_exit(&self.connection), event)
    }

    fn delete_trade_event(&mut self, event_id: Uuid) -> Result<(), Box<dyn Error>> {
        WorkerTradeEvent::delete(&mut lock_connection_or_exit(&self.connection), event_id)
    }
}

impl ReadTradeGradeDB for SqliteDatabase {
    fn read_latest_for_trade(
        &mut self,
        trade_id: Uuid,
    ) -> Result<Option<TradeGrade>, Box<dyn Error>> {
        WorkerTradeGrade::read_latest_for_trade(
            &mut lock_connection_or_exit(&self.connection),
            trade_id,
        )
    }

    fn read_for_account_days(
        &mut self,
        account_id: Uuid,
        days: u32,
    ) -> Result<Vec<TradeGrade>, Box<dyn Error>> {
        WorkerTradeGrade::read_for_account_days(
            &mut lock_connection_or_exit(&self.connection),
            account_id,
            days,
        )
    }
}

impl WriteTradeGradeDB for SqliteDatabase {
    fn create_trade_grade(&mut self, grade: &TradeGrade) -> Result<TradeGrade, Box<dyn Error>> {
        WorkerTradeGrade::create(&mut lock_connection_or_exit(&self.connection), grade)
    }
}

impl ReadLevelDB for SqliteDatabase {
    fn level_for_account(&mut self, account_id: Uuid) -> Result<Level, Box<dyn Error>> {
        WorkerLevel::read_for_account(&mut lock_connection_or_exit(&self.connection), account_id)
    }

    fn level_changes_for_account(
        &mut self,
        account_id: Uuid,
    ) -> Result<Vec<LevelChange>, Box<dyn Error>> {
        WorkerLevel::read_changes_for_account(
            &mut lock_connection_or_exit(&self.connection),
            account_id,
        )
    }

    fn recent_level_changes(
        &mut self,
        account_id: Uuid,
        days: u32,
    ) -> Result<Vec<LevelChange>, Box<dyn Error>> {
        WorkerLevel::read_recent_changes_for_account(
            &mut lock_connection_or_exit(&self.connection),
            account_id,
            days,
        )
    }

    fn level_adjustment_rules_for_account(
        &mut self,
        account_id: Uuid,
    ) -> Result<LevelAdjustmentRules, Box<dyn Error>> {
        WorkerLevel::read_adjustment_rules_for_account(
            &mut lock_connection_or_exit(&self.connection),
            account_id,
        )
    }
}

impl WriteLevelDB for SqliteDatabase {
    fn create_default_level(&mut self, account: &Account) -> Result<Level, Box<dyn Error>> {
        WorkerLevel::create_default(&mut lock_connection_or_exit(&self.connection), account)
    }

    fn update_level(&mut self, level: &Level) -> Result<Level, Box<dyn Error>> {
        WorkerLevel::update(&mut lock_connection_or_exit(&self.connection), level)
    }

    fn create_level_change(
        &mut self,
        level_change: &LevelChange,
    ) -> Result<LevelChange, Box<dyn Error>> {
        WorkerLevel::create_change(&mut lock_connection_or_exit(&self.connection), level_change)
    }

    fn upsert_level_adjustment_rules(
        &mut self,
        account_id: Uuid,
        rules: &LevelAdjustmentRules,
    ) -> Result<LevelAdjustmentRules, Box<dyn Error>> {
        WorkerLevel::upsert_adjustment_rules(
            &mut lock_connection_or_exit(&self.connection),
            account_id,
            rules,
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
            &mut lock_connection_or_exit(&self.connection),
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
            &mut lock_connection_or_exit(&self.connection),
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
            &mut lock_connection_or_exit(&self.connection),
            account_id,
            currency,
        )
    }

    fn all_trade_transactions(
        &mut self,
        trade_id: Uuid,
    ) -> Result<Vec<Transaction>, Box<dyn Error>> {
        WorkerTransaction::read_all_trade_transactions(
            &mut lock_connection_or_exit(&self.connection),
            trade_id,
        )
    }

    fn all_trade_funding_transactions(
        &mut self,
        trade_id: Uuid,
    ) -> Result<Vec<Transaction>, Box<dyn Error>> {
        WorkerTransaction::read_all_trade_transactions_for_category(
            &mut lock_connection_or_exit(&self.connection),
            trade_id,
            TransactionCategory::FundTrade(trade_id),
        )
    }

    fn all_trade_taxes_transactions(
        &mut self,
        trade_id: Uuid,
    ) -> Result<Vec<Transaction>, Box<dyn Error>> {
        WorkerTransaction::read_all_trade_transactions_for_category(
            &mut lock_connection_or_exit(&self.connection),
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
            &mut lock_connection_or_exit(&self.connection),
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
            &mut lock_connection_or_exit(&self.connection),
            account_id,
            currency,
        )
    }
}

impl ReadRuleDB for SqliteDatabase {
    fn read_all_rules(&mut self, account_id: Uuid) -> Result<Vec<Rule>, Box<dyn Error>> {
        WorkerRule::read_all(&mut lock_connection_or_exit(&self.connection), account_id)
    }

    fn rule_for_account(
        &mut self,
        account_id: Uuid,
        name: &RuleName,
    ) -> Result<Rule, Box<dyn Error>> {
        WorkerRule::read_for_account_with_name(
            &mut lock_connection_or_exit(&self.connection),
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
            &mut lock_connection_or_exit(&self.connection),
            name,
            description,
            priority,
            level,
            account,
        )
    }

    fn make_rule_inactive(&mut self, rule: &Rule) -> Result<Rule, Box<dyn Error>> {
        WorkerRule::make_inactive(&mut lock_connection_or_exit(&self.connection), rule)
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
            &mut lock_connection_or_exit(&self.connection),
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
        WorkerTradingVehicle::upsert(&mut lock_connection_or_exit(&self.connection), input)
    }
}

impl ReadTradingVehicleDB for SqliteDatabase {
    fn read_all_trading_vehicles(&mut self) -> Result<Vec<TradingVehicle>, Box<dyn Error>> {
        WorkerTradingVehicle::read_all(&mut lock_connection_or_exit(&self.connection))
    }

    fn read_trading_vehicle(&mut self, id: Uuid) -> Result<TradingVehicle, Box<dyn Error>> {
        WorkerTradingVehicle::read(&mut lock_connection_or_exit(&self.connection), id)
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
            &mut lock_connection_or_exit(&self.connection),
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
            &mut lock_connection_or_exit(&self.connection),
            status,
            trade,
        )
    }
}

impl ReadTradeDB for SqliteDatabase {
    fn read_trade(&mut self, id: Uuid) -> Result<Trade, Box<dyn Error>> {
        WorkerTrade::read_trade(&mut lock_connection_or_exit(&self.connection), id)
    }

    fn read_trade_status(&mut self, id: Uuid) -> Result<Status, Box<dyn Error>> {
        WorkerTrade::read_trade_status(&mut lock_connection_or_exit(&self.connection), id)
    }

    fn read_trade_balance(&mut self, balance_id: Uuid) -> Result<TradeBalance, Box<dyn Error>> {
        WorkerTrade::read_balance(&mut lock_connection_or_exit(&self.connection), balance_id)
    }

    fn all_open_trades_for_currency(
        &mut self,
        account_id: Uuid,
        currency: &Currency,
    ) -> Result<Vec<Trade>, Box<dyn Error>> {
        WorkerTrade::read_all_funded_trades_for_currency(
            &mut lock_connection_or_exit(&self.connection),
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
            &mut lock_connection_or_exit(&self.connection),
            account_id,
            status,
        )
    }

    fn read_recent_closed_trade_performances(
        &mut self,
        account_id: Uuid,
        currency: &Currency,
        cutoff: chrono::NaiveDateTime,
    ) -> Result<Vec<model::ClosedTradePerformance>, Box<dyn Error>> {
        WorkerTrade::read_recent_closed_trade_performances(
            &mut lock_connection_or_exit(&self.connection),
            account_id,
            currency,
            cutoff,
        )
    }

    fn read_recent_closed_trade_performance_points(
        &mut self,
        account_id: Uuid,
        currency: &Currency,
        cutoff: chrono::NaiveDateTime,
    ) -> Result<Vec<(chrono::NaiveDateTime, rust_decimal::Decimal)>, Box<dyn Error>> {
        WorkerTrade::read_recent_closed_trade_performance_points(
            &mut lock_connection_or_exit(&self.connection),
            account_id,
            currency,
            cutoff,
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
            &mut lock_connection_or_exit(&self.connection),
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
        WorkerOrder::read(&mut lock_connection_or_exit(&self.connection), id)
    }
}

impl ReadExecutionDB for SqliteDatabase {
    fn all_trade_executions(&mut self, trade_id: Uuid) -> Result<Vec<Execution>, Box<dyn Error>> {
        WorkerExecution::read_for_trade(&mut lock_connection_or_exit(&self.connection), trade_id)
    }

    fn all_order_executions(&mut self, order_id: Uuid) -> Result<Vec<Execution>, Box<dyn Error>> {
        WorkerExecution::read_for_order(&mut lock_connection_or_exit(&self.connection), order_id)
    }

    fn latest_trade_execution_at(
        &mut self,
        trade_id: Uuid,
    ) -> Result<Option<chrono::NaiveDateTime>, Box<dyn Error>> {
        WorkerExecution::latest_for_trade(&mut lock_connection_or_exit(&self.connection), trade_id)
    }
}

impl WriteExecutionDB for SqliteDatabase {
    fn upsert_execution(&mut self, execution: &Execution) -> Result<Execution, Box<dyn Error>> {
        WorkerExecution::upsert(&mut lock_connection_or_exit(&self.connection), execution)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Duration, NaiveDate, Utc};
    use model::{
        Account, Environment, ExecutionSide, ExecutionSource, Grade, LevelTrigger,
        MistakeErrorType, MungerTendency, RuleLevel, RuleName, SessionRegime, TradeCategory,
        TradeEventSeverity, TradeEventSource, TradeEventType,
    };
    use rust_decimal_macros::dec;
    use std::path::PathBuf;

    fn unique_temp_path(label: &str, extension: &str) -> PathBuf {
        std::env::temp_dir().join(format!("trust-{label}-{}.{}", Uuid::new_v4(), extension))
    }

    fn create_account(database: &SqliteDatabase, name: &str) -> Account {
        database
            .account_write()
            .create(name, name, Environment::Paper, dec!(0), dec!(0))
            .expect("account should be created")
    }

    fn create_vehicle(database: &SqliteDatabase, symbol: &str) -> TradingVehicle {
        database
            .trading_vehicle_write()
            .create_trading_vehicle(
                symbol,
                Some(symbol),
                &TradingVehicleCategory::Stock,
                "alpaca",
            )
            .expect("trading vehicle should be created")
    }

    fn create_order(
        database: &SqliteDatabase,
        vehicle: &TradingVehicle,
        action: OrderAction,
        category: OrderCategory,
        price: Decimal,
    ) -> Order {
        database
            .order_write()
            .create(vehicle, dec!(10), price, &Currency::USD, &action, &category)
            .expect("order should be created")
    }

    fn create_trade_graph(database: &SqliteDatabase, account: &Account, symbol: &str) -> Trade {
        let vehicle = create_vehicle(database, symbol);
        let stop = create_order(
            database,
            &vehicle,
            OrderAction::Sell,
            OrderCategory::Stop,
            dec!(90),
        );
        let entry = create_order(
            database,
            &vehicle,
            OrderAction::Buy,
            OrderCategory::Limit,
            dec!(100),
        );
        let target = create_order(
            database,
            &vehicle,
            OrderAction::Sell,
            OrderCategory::Limit,
            dec!(120),
        );
        let draft = DraftTrade {
            account: account.clone(),
            trading_vehicle: vehicle,
            quantity: 10.into(),
            currency: Currency::USD,
            category: TradeCategory::Long,
            thesis: Some("database facade route".to_string()),
            sector: Some("technology".to_string()),
            asset_class: Some("equity".to_string()),
            context: Some("unit test".to_string()),
        };

        database
            .trade_write()
            .create_trade(draft, &stop, &entry, &target)
            .expect("trade should be created")
    }

    fn assert_single_trade(trades: Vec<Trade>, trade_id: Uuid) {
        let mut trades = trades.iter();
        let trade = trades.next().expect("one trade should be returned");
        assert!(trades.next().is_none());
        assert_eq!(trade.id, trade_id);
    }

    fn trade_grade_for(trade: &Trade) -> TradeGrade {
        let now = Utc::now().naive_utc();
        TradeGrade {
            id: Uuid::new_v4(),
            created_at: now,
            updated_at: now,
            deleted_at: None,
            trade_id: trade.id,
            overall_score: 88,
            overall_grade: Grade::BPlus,
            process_score: 90,
            risk_score: 87,
            execution_score: 86,
            documentation_score: 89,
            recommendations: vec!["tighten entry notes".to_string()],
            graded_at: now,
            process_weight_permille: 250,
            risk_weight_permille: 300,
            execution_weight_permille: 250,
            documentation_weight_permille: 200,
        }
    }

    fn trade_event_for(trade: &Trade) -> TradeEvent {
        let now = Utc::now().naive_utc();
        TradeEvent {
            id: Uuid::new_v4(),
            created_at: now,
            updated_at: now,
            deleted_at: None,
            trade_id: trade.id,
            symbol: trade.trading_vehicle.symbol.clone(),
            event_type: TradeEventType::Earnings,
            event_date: NaiveDate::from_ymd_opt(2026, 1, 20).expect("fixture date should be valid"),
            severity: TradeEventSeverity::High,
            notes: Some("fixture event".to_string()),
            source: TradeEventSource::Manual,
        }
    }

    fn mistake_for(trade: &Trade) -> Mistake {
        let now = Utc::now().naive_utc();
        Mistake {
            id: Uuid::new_v4(),
            created_at: now,
            updated_at: now,
            deleted_at: None,
            trade_id: trade.id,
            bias_tags: vec![
                MungerTendency::InconsistencyAvoidance,
                MungerTendency::DeprivalSuperreaction,
            ],
            lollapalooza: true,
            error_type: MistakeErrorType::Commission,
            rule_violated: Some("move_stop_only_to_reduce_risk".to_string()),
            counterfactual_r: dec!(1.75),
            lesson: "Pre-commit stop movement criteria before entry.".to_string(),
        }
    }

    fn session_plan_for(account: &Account) -> SessionPlan {
        let now = Utc::now().naive_utc();
        SessionPlan {
            id: Uuid::new_v4(),
            created_at: now,
            updated_at: now,
            deleted_at: None,
            account_id: account.id,
            opened_at: now,
            closed_at: None,
            regime: SessionRegime::Normal,
            permitted_setups: vec!["opening range".to_string(), "pullback".to_string()],
            max_positions: 2,
            hypothesis: "follow planned setups only".to_string(),
            success_criteria: "take valid setups only".to_string(),
            failure_criteria: "force trades outside plan".to_string(),
            session_grade: None,
            adherence_notes: None,
        }
    }

    fn assert_account_balance_facade(database: &SqliteDatabase, account: &Account) {
        let balance = database
            .account_balance_write()
            .create(account, &Currency::USD)
            .expect("balance should be created");
        let updated = database
            .account_balance_write()
            .update(&balance, dec!(500), dec!(25), dec!(475), dec!(10))
            .expect("balance should be updated");
        let read = database
            .account_balance_read()
            .for_currency(account.id, &Currency::USD)
            .expect("balance should read");

        assert_eq!(updated.total_balance, dec!(500));
        assert_eq!(read.total_available, dec!(475));
        assert_eq!(read.taxed, dec!(10));
    }

    fn assert_rule_facade(database: &SqliteDatabase, account: &Account) {
        let name = RuleName::RiskPerTrade(2.5);
        let level = RuleLevel::Error;
        let created = database
            .rule_write()
            .create_rule(account, &name, "risk limit", 1, &level)
            .expect("rule should be created");
        let found = database
            .rule_read()
            .rule_for_account(account.id, &name)
            .expect("rule should read by name");
        let active_rules = database
            .rule_read()
            .read_all_rules(account.id)
            .expect("active rules should read");
        let inactive = database
            .rule_write()
            .make_rule_inactive(&created)
            .expect("rule should be deactivated");
        let active_rules_after_deactivation = database
            .rule_read()
            .read_all_rules(account.id)
            .expect("active rules should read after deactivation");

        assert_eq!(found.id, created.id);
        assert!(active_rules.iter().any(|rule| rule.id == created.id));
        assert!(!inactive.active);
        assert!(!active_rules_after_deactivation
            .iter()
            .any(|rule| rule.id == created.id));
    }

    fn assert_distribution_and_advisory_facades(database: &SqliteDatabase, account: &Account) {
        database
            .advisory_write()
            .upsert_advisory_thresholds(account.id, dec!(35), dec!(45), dec!(20))
            .expect("advisory thresholds should upsert");
        let thresholds = database
            .advisory_read()
            .advisory_thresholds_for_account(account.id)
            .expect("advisory thresholds should read");
        assert_eq!(thresholds, Some((dec!(35), dec!(45), dec!(20))));

        let rules = database
            .distribution_write()
            .create_or_update(
                account.id,
                dec!(0.4),
                dec!(0.3),
                dec!(0.3),
                dec!(100),
                "hash",
            )
            .expect("distribution rules should upsert");
        let history = database
            .distribution_write()
            .create_history(
                account.id,
                None,
                dec!(250),
                Utc::now().naive_utc(),
                Some(dec!(100)),
                Some(dec!(75)),
                Some(dec!(75)),
            )
            .expect("distribution history should write");
        let read_rules = database
            .distribution_read()
            .for_account(account.id)
            .expect("distribution rules should read");
        let read_history = database
            .distribution_read()
            .history_for_account(account.id)
            .expect("distribution history should read");

        assert_eq!(read_rules.id, rules.id);
        assert_eq!(read_rules.insurance_percent, Decimal::ZERO);
        assert!(read_history.iter().any(|entry| entry.id == history.id));
    }

    fn assert_level_facade(database: &SqliteDatabase, account: &Account) {
        let level = database
            .level_write()
            .create_default_level(account)
            .expect("default level should be created");
        let now = Utc::now().naive_utc();
        let (transitioned, change) = level
            .transition_to(2, "risk review", LevelTrigger::ManualReview, now)
            .expect("level transition should build");
        let updated = database
            .level_write()
            .update_level(&transitioned)
            .expect("level should update");
        let written_change = database
            .level_write()
            .create_level_change(&change)
            .expect("level change should write");
        let rules = database
            .level_write()
            .upsert_level_adjustment_rules(account.id, &LevelAdjustmentRules::default())
            .expect("level rules should upsert");

        assert_eq!(updated.current_level, 2);
        assert_eq!(
            database
                .level_read()
                .level_for_account(account.id)
                .expect("level should read")
                .id,
            level.id
        );
        assert_eq!(
            database
                .level_read()
                .level_adjustment_rules_for_account(account.id)
                .expect("level rules should read"),
            rules
        );
        assert!(database
            .level_read()
            .level_changes_for_account(account.id)
            .expect("level changes should read")
            .iter()
            .any(|entry| entry.id == written_change.id));
        assert!(database
            .level_read()
            .recent_level_changes(account.id, 1)
            .expect("recent level changes should read")
            .iter()
            .any(|entry| entry.id == written_change.id));
    }

    fn assert_order_facade(database: &SqliteDatabase, order: &Order) -> Order {
        let submitted = database
            .order_write()
            .submit_of(order, "broker-entry".to_string())
            .expect("order should submit");
        let filled = database
            .order_write()
            .filling_of(&submitted)
            .expect("order should fill");
        let closed = database
            .order_write()
            .closing_of(&filled)
            .expect("order should close");
        let updated = database
            .order_write()
            .update(&closed)
            .expect("order should update");
        let read = database
            .order_read()
            .for_id(updated.id)
            .expect("order should read");

        assert_eq!(read.broker_order_id.as_deref(), Some("broker-entry"));
        assert!(read.submitted_at.is_some());
        assert!(read.filled_at.is_some());
        assert!(read.closed_at.is_some());
        updated
    }

    fn assert_trade_facade(database: &SqliteDatabase, account: &Account, trade: &Trade) -> Trade {
        let read = database
            .trade_read()
            .read_trade(trade.id)
            .expect("trade should read");
        let funded = database
            .trade_write()
            .update_trade_status(Status::Funded, &read)
            .expect("trade should fund");
        let balance = database
            .trade_balance_write()
            .update_trade_balance(
                &funded,
                dec!(1000),
                dec!(900),
                dec!(100),
                dec!(30),
                dec!(75),
            )
            .expect("trade balance should update");

        assert_eq!(
            database
                .trade_read()
                .read_trade_status(trade.id)
                .expect("trade status should read"),
            Status::Funded
        );
        assert_single_trade(
            database
                .trade_read()
                .all_open_trades_for_currency(account.id, &Currency::USD)
                .expect("open trades should read"),
            trade.id,
        );
        assert_single_trade(
            database
                .trade_read()
                .read_trades_with_status(account.id, Status::Funded)
                .expect("funded trades should read"),
            trade.id,
        );
        assert_eq!(
            database
                .trade_read()
                .read_trade_balance(balance.id)
                .expect("trade balance should read")
                .total_performance,
            dec!(75)
        );
        let mut funded_with_balance = funded;
        funded_with_balance.balance = balance;
        let closed = database
            .trade_write()
            .update_trade_status(Status::ClosedTarget, &funded_with_balance)
            .expect("trade should close");
        let cutoff = closed
            .updated_at
            .checked_sub_signed(Duration::seconds(1))
            .expect("one-second cutoff before closed trade timestamp should be representable");
        let closed_performances = database
            .trade_read()
            .read_recent_closed_trade_performances(account.id, &Currency::USD, cutoff)
            .expect("closed trade performances should read");
        assert!(closed_performances.iter().any(|performance| {
            performance.trade_id == trade.id && performance.total_performance == dec!(75)
        }));
        closed
    }

    fn assert_log_execution_and_grade_facades(
        database: &SqliteDatabase,
        account: &Account,
        trade: &Trade,
        order: &Order,
    ) {
        let log = database
            .log_write()
            .create_log("submitted", trade)
            .expect("broker log should write");
        assert!(database
            .log_read()
            .read_all_logs_for_trade(trade.id)
            .expect("broker logs should read")
            .iter()
            .any(|entry| entry.id == log.id));

        let executed_at = Utc::now().naive_utc();
        let mut execution = Execution::new(
            "alpaca".to_string(),
            ExecutionSource::TradeUpdates,
            account.id,
            "exec-1".to_string(),
            Some("broker-entry".to_string()),
            trade.trading_vehicle.symbol.clone(),
            ExecutionSide::Buy,
            dec!(10),
            dec!(101),
            executed_at,
        );
        execution.trade_id = Some(trade.id);
        execution.order_id = Some(order.id);
        let written = database
            .execution_write()
            .upsert_execution(&execution)
            .expect("execution should write");
        assert_eq!(
            database
                .execution_read()
                .latest_trade_execution_at(trade.id)
                .expect("latest execution should read"),
            Some(executed_at)
        );
        assert!(database
            .execution_read()
            .all_trade_executions(trade.id)
            .expect("trade executions should read")
            .iter()
            .any(|entry| entry.id == written.id));
        assert!(database
            .execution_read()
            .all_order_executions(order.id)
            .expect("order executions should read")
            .iter()
            .any(|entry| entry.id == written.id));

        let grade = database
            .trade_grade_write()
            .create_trade_grade(&trade_grade_for(trade))
            .expect("trade grade should write");
        assert_eq!(
            database
                .trade_grade_read()
                .read_latest_for_trade(trade.id)
                .expect("latest grade should read")
                .expect("latest grade should exist")
                .id,
            grade.id
        );
        assert!(database
            .trade_grade_read()
            .read_for_account_days(account.id, 1)
            .expect("account grades should read")
            .iter()
            .any(|entry| entry.id == grade.id));

        assert_session_plan_facade(database, account);
        assert_mistake_facade(database, account, trade);
        assert_trade_event_facade(database, trade);
    }

    fn assert_session_plan_facade(database: &SqliteDatabase, account: &Account) {
        let plan = database
            .session_plan_write()
            .create_session_plan(&session_plan_for(account))
            .expect("session plan should write");
        assert_eq!(
            database
                .session_plan_read()
                .read_open_session(account.id)
                .expect("open session should read")
                .map(|entry| entry.id),
            Some(plan.id)
        );

        let window = Duration::seconds(1);
        let start_at = plan
            .opened_at
            .checked_sub_signed(window)
            .expect("session timestamp should support start window");
        let closed_at = plan
            .opened_at
            .checked_add_signed(window)
            .expect("session timestamp should support close window");
        let closed = database
            .session_plan_write()
            .close_session_plan(&SessionPlanClose {
                session_plan_id: plan.id,
                closed_at,
                session_grade: Some("A".to_string()),
                adherence_notes: Some("followed plan".to_string()),
            })
            .expect("session plan should close");
        assert_eq!(closed.session_grade.as_deref(), Some("A"));
        assert!(database
            .session_plan_read()
            .read_session_plans_for_account(account.id, start_at, closed_at)
            .expect("account session plans should read")
            .iter()
            .any(|entry| entry.id == plan.id));
        assert!(database
            .session_plan_read()
            .read_open_session(account.id)
            .expect("open session should read after close")
            .is_none());
    }

    fn assert_mistake_facade(database: &SqliteDatabase, account: &Account, trade: &Trade) {
        let mistake = database
            .mistake_write()
            .create_mistake(&mistake_for(trade))
            .expect("mistake should write");
        let window = Duration::seconds(1);
        let start_at = mistake
            .created_at
            .checked_sub_signed(window)
            .expect("mistake timestamp should support start window");
        let end_at = mistake
            .created_at
            .checked_add_signed(window)
            .expect("mistake timestamp should support end window");
        assert!(database
            .mistake_read()
            .read_mistakes_for_trade(trade.id)
            .expect("trade mistakes should read")
            .iter()
            .any(|entry| entry.id == mistake.id));
        assert!(database
            .mistake_read()
            .read_mistakes_for_account_in_period(account.id, start_at, end_at)
            .expect("account period mistakes should read")
            .iter()
            .any(|entry| entry.id == mistake.id));
    }

    fn assert_trade_event_facade(database: &SqliteDatabase, trade: &Trade) {
        let event = database
            .trade_event_write()
            .create_trade_event(&trade_event_for(trade))
            .expect("trade event should write");
        assert!(database
            .trade_event_read()
            .read_trade_events_for_trade(trade.id)
            .expect("trade events should read")
            .iter()
            .any(|entry| entry.id == event.id));
        database
            .trade_event_write()
            .delete_trade_event(event.id)
            .expect("trade event should delete");
        assert!(!database
            .trade_event_read()
            .read_trade_events_for_trade(trade.id)
            .expect("trade events should read after delete")
            .iter()
            .any(|entry| entry.id == event.id));
    }

    #[test]
    fn savepoints_validate_names_and_rollback_inner_work() {
        let mut database = SqliteDatabase::new_in_memory();
        assert!(SqliteDatabase::validate_savepoint_name("risk_checkpoint_1").is_ok());
        for invalid in ["", "risk-checkpoint", "risk checkpoint", "risk;drop", "å"] {
            assert!(SqliteDatabase::validate_savepoint_name(invalid).is_err());
        }

        database
            .begin_savepoint("outer_checkpoint")
            .expect("outer savepoint should begin");
        let kept = create_account(&database, "savepoint-kept");
        database
            .begin_savepoint("inner_checkpoint")
            .expect("inner savepoint should begin");
        let rolled_back = create_account(&database, "savepoint-rolled-back");

        database
            .rollback_to_savepoint("inner_checkpoint")
            .expect("inner savepoint should roll back");
        database
            .release_savepoint("inner_checkpoint")
            .expect("inner savepoint should release after rollback");
        assert!(database.account_read().id(kept.id).is_ok());
        assert!(database.account_read().id(rolled_back.id).is_err());
        database
            .release_savepoint("outer_checkpoint")
            .expect("outer savepoint should release");
    }

    #[test]
    fn factory_objects_share_connection_for_account_policy_and_distribution_data() {
        let database = SqliteDatabase::new_in_memory();
        let account = create_account(&database, "database-factory-account");
        assert_eq!(
            database
                .account_read()
                .for_name("database-factory-account")
                .expect("account should read by name")
                .id,
            account.id
        );

        assert_account_balance_facade(&database, &account);
        assert_rule_facade(&database, &account);
        assert_distribution_and_advisory_facades(&database, &account);
        assert_level_facade(&database, &account);
    }

    #[test]
    fn factory_objects_share_connection_for_trade_order_execution_and_grade_data() {
        let database = SqliteDatabase::new_in_memory();
        let account = create_account(&database, "database-trade-account");
        let trade = create_trade_graph(&database, &account, "DBFACADE");
        let updated_order = assert_order_facade(&database, &trade.entry);
        let repriced = database
            .order_write()
            .update_price(&trade.target, dec!(125), "broker-target".to_string())
            .expect("target price should update");
        let closed = assert_trade_facade(&database, &account, &trade);

        assert_eq!(updated_order.id, trade.entry.id);
        assert_eq!(repriced.unit_price, dec!(125));
        assert_eq!(
            database
                .trading_vehicle_read()
                .read_trading_vehicle(trade.trading_vehicle.id)
                .expect("vehicle should read")
                .id,
            trade.trading_vehicle.id
        );
        assert!(database
            .trading_vehicle_read()
            .read_all_trading_vehicles()
            .expect("vehicles should read")
            .iter()
            .any(|vehicle| vehicle.id == trade.trading_vehicle.id));
        assert_log_execution_and_grade_facades(&database, &account, &closed, &updated_order);
    }

    #[test]
    fn transaction_facade_transfer_pair_is_atomic_and_queryable() {
        let database = SqliteDatabase::new_in_memory();
        let source = create_account(&database, "database-transfer-source");
        let destination = create_account(&database, "database-transfer-destination");

        let deposit = database
            .transaction_write()
            .create_transaction_by_account_id(
                source.id,
                dec!(500),
                &Currency::USD,
                TransactionCategory::Deposit,
            )
            .expect("deposit should write");
        let (withdrawal, child_deposit) = database
            .transaction_write()
            .create_transfer_pair(
                &source,
                &destination,
                dec!(125),
                &Currency::USD,
                TransactionCategory::Withdrawal,
                TransactionCategory::Deposit,
            )
            .expect("transfer pair should write");

        let source_transactions = database
            .transaction_read()
            .all_transactions(source.id, &Currency::USD)
            .expect("source transactions should read");
        assert!(source_transactions
            .iter()
            .any(|transaction| transaction.id == deposit.id));
        assert!(source_transactions
            .iter()
            .any(|transaction| transaction.id == withdrawal.id));
        assert!(database
            .transaction_read()
            .all_account_transactions_excluding_taxes(source.id, &Currency::USD)
            .expect("non-tax transactions should read")
            .iter()
            .any(|transaction| transaction.id == deposit.id));
        assert!(database
            .transaction_read()
            .all_transactions(destination.id, &Currency::USD)
            .expect("destination transactions should read")
            .iter()
            .any(|transaction| transaction.id == child_deposit.id));
        assert!(database
            .transaction_read()
            .read_all_account_transactions_taxes(source.id, &Currency::USD)
            .expect("tax transactions should read")
            .is_empty());
        assert!(database
            .transaction_read()
            .all_account_transactions_funding_in_submitted_trades(source.id, &Currency::USD)
            .expect("funding-in-trades transactions should read")
            .is_empty());
    }

    #[test]
    fn file_database_debug_and_backup_wrappers_roundtrip() {
        let db_path = unique_temp_path("sqlite-source", "db");
        let backup_path = unique_temp_path("sqlite-backup", "json");

        let database = SqliteDatabase::new(db_path.to_str().expect("temporary path is utf-8"));
        assert!(format!("{database:?}").contains("Arc<Mutex<SqliteConnection>>"));

        let account = create_account(&database, "database-backup-wrapper");
        let existing = SqliteDatabase::new(db_path.to_str().expect("temporary path is utf-8"));
        assert_eq!(
            existing
                .account_read()
                .id(account.id)
                .expect("existing file database should read account")
                .id,
            account.id
        );
        database
            .export_backup_to_path(&backup_path)
            .expect("backup export should succeed");

        let mut imported = SqliteDatabase::new_in_memory();
        let report = imported
            .import_backup_from_path(&backup_path, ImportOptions::default())
            .expect("backup import should succeed");

        assert!(report.inserted_rows > 0);
        assert_eq!(
            imported
                .account_read()
                .id(account.id)
                .expect("imported account should read")
                .name,
            "database-backup-wrapper"
        );

        let _ = std::fs::remove_file(&db_path);
        let _ = std::fs::remove_file(&backup_path);
    }
}
