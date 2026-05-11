use crate::{
    Account, AccountBalance, AccountType, BrokerKind, BrokerLog, Currency,
    DistributionExecutionPlan, DistributionHistory, DistributionRules, Environment, Execution,
    Level, LevelAdjustmentRules, LevelChange, Mistake, Order, OrderAction, OrderCategory, Rule,
    RuleLevel, RuleName, SessionPlan, SessionPlanClose, Status, Trade, TradeBalance, TradeCategory,
    TradeEvent, TradeGrade, TradingVehicle, TradingVehicleCategory, Transaction,
    TransactionCategory,
};
use rust_decimal::Decimal;
use uuid::Uuid;

use std::error::Error;

/// Database trait with all the methods that are needed to interact with the database.
///
/// The trait is used to abstract the database implementation.
/// The trait is used to:
///
/// 1. Make it easier to switch the database implementation.
/// 2. Easier to test the code.
/// 3. Easier to mock the database.
///
/// To prevent the database from being used incorrectly, the trait has the following rules:
/// - Reads can be Uuid
/// - Writes and updates must be Domain Models
pub trait DatabaseFactory {
    /// Returns a reader for account data operations
    fn account_read(&self) -> Box<dyn AccountRead>;
    /// Returns a writer for account data operations
    fn account_write(&self) -> Box<dyn AccountWrite>;
    /// Returns a reader for account balance data operations
    fn account_balance_read(&self) -> Box<dyn AccountBalanceRead>;
    /// Returns a writer for account balance data operations
    fn account_balance_write(&self) -> Box<dyn AccountBalanceWrite>;
    /// Returns a reader for order data operations
    fn order_read(&self) -> Box<dyn OrderRead>;
    /// Returns a writer for order data operations
    fn order_write(&self) -> Box<dyn OrderWrite>;
    /// Returns a reader for transaction data operations
    fn transaction_read(&self) -> Box<dyn ReadTransactionDB>;
    /// Returns a writer for transaction data operations
    fn transaction_write(&self) -> Box<dyn WriteTransactionDB>;
    /// Returns a reader for trade data operations
    fn trade_read(&self) -> Box<dyn ReadTradeDB>;
    /// Returns a writer for trade data operations
    fn trade_write(&self) -> Box<dyn WriteTradeDB>;
    /// Returns a writer for trade balance data operations
    fn trade_balance_write(&self) -> Box<dyn WriteAccountBalanceDB>;
    /// Returns a reader for rule data operations
    fn rule_read(&self) -> Box<dyn ReadRuleDB>;
    /// Returns a writer for rule data operations
    fn rule_write(&self) -> Box<dyn WriteRuleDB>;
    /// Returns a reader for trading vehicle data operations
    fn trading_vehicle_read(&self) -> Box<dyn ReadTradingVehicleDB>;
    /// Returns a writer for trading vehicle data operations
    fn trading_vehicle_write(&self) -> Box<dyn WriteTradingVehicleDB>;
    /// Returns a reader for broker log data operations
    fn log_read(&self) -> Box<dyn ReadBrokerLogsDB>;
    /// Returns a writer for broker log data operations
    fn log_write(&self) -> Box<dyn WriteBrokerLogsDB>;
    /// Returns a reader for execution data operations
    fn execution_read(&self) -> Box<dyn ReadExecutionDB>;
    /// Returns a writer for execution data operations
    fn execution_write(&self) -> Box<dyn WriteExecutionDB>;
    /// Returns a reader for mistake data operations
    fn mistake_read(&self) -> Box<dyn ReadMistakeDB>;
    /// Returns a writer for mistake data operations
    fn mistake_write(&self) -> Box<dyn WriteMistakeDB>;
    /// Returns a reader for session plan operations
    fn session_plan_read(&self) -> Box<dyn ReadSessionPlanDB>;
    /// Returns a writer for session plan operations
    fn session_plan_write(&self) -> Box<dyn WriteSessionPlanDB>;
    /// Returns a reader for trade event data operations
    fn trade_event_read(&self) -> Box<dyn ReadTradeEventDB>;
    /// Returns a writer for trade event data operations
    fn trade_event_write(&self) -> Box<dyn WriteTradeEventDB>;
    /// Returns a reader for trade grade data operations
    fn trade_grade_read(&self) -> Box<dyn ReadTradeGradeDB>;
    /// Returns a writer for trade grade data operations
    fn trade_grade_write(&self) -> Box<dyn WriteTradeGradeDB>;
    /// Returns a reader for level data operations
    fn level_read(&self) -> Box<dyn ReadLevelDB>;
    /// Returns a writer for level data operations
    fn level_write(&self) -> Box<dyn WriteLevelDB>;

    /// Begins a named savepoint.
    ///
    /// Savepoints can be nested and are compatible with existing outer transactions.
    fn begin_savepoint(&mut self, name: &str) -> Result<(), Box<dyn Error>>;

    /// Releases a previously opened named savepoint.
    fn release_savepoint(&mut self, name: &str) -> Result<(), Box<dyn Error>>;

    /// Rolls back all changes after a named savepoint.
    fn rollback_to_savepoint(&mut self, name: &str) -> Result<(), Box<dyn Error>>;

    /// Returns a reader for distribution rules data operations
    fn distribution_read(&self) -> Box<dyn DistributionRead>;
    /// Returns a writer for distribution rules data operations
    fn distribution_write(&self) -> Box<dyn DistributionWrite>;

    /// Returns a reader for advisory threshold operations
    fn advisory_read(&self) -> Box<dyn AdvisoryRead>;
    /// Returns a writer for advisory threshold operations
    fn advisory_write(&self) -> Box<dyn AdvisoryWrite>;
}

/// Trait for reading account data from the database
pub trait AccountRead {
    /// Retrieves an account by its name
    fn for_name(&mut self, name: &str) -> Result<Account, Box<dyn Error>>;
    /// Retrieves an account by its ID
    fn id(&mut self, id: Uuid) -> Result<Account, Box<dyn Error>>;
    /// Retrieves all accounts from the database
    fn all(&mut self) -> Result<Vec<Account>, Box<dyn Error>>;
}

/// Trait for writing account data to the database
pub trait AccountWrite {
    /// Creates a new account with the specified parameters
    fn create(
        &mut self,
        name: &str,
        description: &str,
        environment: Environment,
        taxes_percentage: Decimal,
        earnings_percentage: Decimal,
    ) -> Result<Account, Box<dyn Error>>;

    /// Creates a new account with hierarchy metadata.
    ///
    /// Default implementation falls back to `create` for DBs that haven't
    /// implemented hierarchy persistence yet.
    #[allow(clippy::too_many_arguments)]
    fn create_with_hierarchy(
        &mut self,
        name: &str,
        description: &str,
        environment: Environment,
        taxes_percentage: Decimal,
        earnings_percentage: Decimal,
        _account_type: AccountType,
        _parent_account_id: Option<Uuid>,
    ) -> Result<Account, Box<dyn Error>> {
        self.create(
            name,
            description,
            environment,
            taxes_percentage,
            earnings_percentage,
        )
    }

    /// Creates a new account with hierarchy and broker profile metadata.
    #[allow(clippy::too_many_arguments)]
    fn create_with_profile(
        &mut self,
        name: &str,
        description: &str,
        environment: Environment,
        taxes_percentage: Decimal,
        earnings_percentage: Decimal,
        account_type: AccountType,
        parent_account_id: Option<Uuid>,
        _broker_kind: BrokerKind,
        _broker_account_id: Option<&str>,
    ) -> Result<Account, Box<dyn Error>> {
        self.create_with_hierarchy(
            name,
            description,
            environment,
            taxes_percentage,
            earnings_percentage,
            account_type,
            parent_account_id,
        )
    }

    /// Soft-deletes an account after implementation-specific safety checks.
    ///
    /// `force` may bypass zero-balance checks, but implementations should still
    /// protect account hierarchy integrity and open trade state.
    fn delete(&mut self, account_id: Uuid, _force: bool) -> Result<Account, Box<dyn Error>> {
        Err(format!(
            "account deletion is not supported by this database implementation: {account_id}"
        )
        .into())
    }
}

/// Trait for reading account balance data from the database
pub trait AccountBalanceRead {
    /// Retrieves all account balances for a specific account
    fn for_account(&mut self, account_id: Uuid) -> Result<Vec<AccountBalance>, Box<dyn Error>>;

    /// Retrieves the account balance for a specific currency
    fn for_currency(
        &mut self,
        account_id: Uuid,
        currency: &Currency,
    ) -> Result<AccountBalance, Box<dyn Error>>;
}

/// Trait for writing account balance data to the database
pub trait AccountBalanceWrite {
    /// Creates a new account balance entry for the given account and currency
    fn create(
        &mut self,
        account: &Account,
        currency: &Currency,
    ) -> Result<AccountBalance, Box<dyn Error>>;

    /// Updates an existing account balance with new values
    fn update(
        &mut self,
        balance: &AccountBalance,
        balance: Decimal,
        in_trade: Decimal,
        available: Decimal,
        taxed: Decimal,
    ) -> Result<AccountBalance, Box<dyn Error>>;
}

/// Trait for reading order data from the database
pub trait OrderRead {
    /// Retrieves an order by its ID
    fn for_id(&mut self, id: Uuid) -> Result<Order, Box<dyn Error>>;
}

/// Trait for writing order data to the database
pub trait OrderWrite {
    /// Creates a new order with the specified parameters
    fn create(
        &mut self,
        trading_vehicle: &TradingVehicle,
        quantity: i64,
        price: Decimal,
        currency: &Currency,
        action: &OrderAction,
        category: &OrderCategory,
    ) -> Result<Order, Box<dyn Error>>;
    /// Marks an order as submitted with the broker's order ID
    fn submit_of(
        &mut self,
        order: &Order,
        broker_order_id: String,
    ) -> Result<Order, Box<dyn Error>>;
    /// Marks an order as being filled
    fn filling_of(&mut self, order: &Order) -> Result<Order, Box<dyn Error>>;
    /// Marks an order as closed
    fn closing_of(&mut self, order: &Order) -> Result<Order, Box<dyn Error>>;
    /// Updates an existing order
    fn update(&mut self, order: &Order) -> Result<Order, Box<dyn Error>>;
    /// Updates the price of an order with the broker's ID
    fn update_price(
        &mut self,
        order: &Order,
        price: Decimal,
        broker_id: String,
    ) -> Result<Order, Box<dyn Error>>;
}

/// Trait for reading transaction data from the database
pub trait ReadTransactionDB {
    /// Retrieves all account transactions excluding tax transactions
    fn all_account_transactions_excluding_taxes(
        &mut self,
        account_id: Uuid,
        currency: &Currency,
    ) -> Result<Vec<Transaction>, Box<dyn Error>>;

    /// Retrieves all account transactions that are funding submitted trades
    fn all_account_transactions_funding_in_submitted_trades(
        &mut self,
        account_id: Uuid,
        currency: &Currency,
    ) -> Result<Vec<Transaction>, Box<dyn Error>>;

    /// Retrieves all tax-related transactions for an account
    fn read_all_account_transactions_taxes(
        &mut self,
        account_id: Uuid,
        currency: &Currency,
    ) -> Result<Vec<Transaction>, Box<dyn Error>>;

    /// Retrieves all transactions associated with a specific trade
    fn all_trade_transactions(
        &mut self,
        trade_id: Uuid,
    ) -> Result<Vec<Transaction>, Box<dyn Error>>;

    /// Retrieves all funding transactions for a specific trade
    fn all_trade_funding_transactions(
        &mut self,
        trade_id: Uuid,
    ) -> Result<Vec<Transaction>, Box<dyn Error>>;

    /// Retrieves all tax transactions for a specific trade
    fn all_trade_taxes_transactions(
        &mut self,
        trade_id: Uuid,
    ) -> Result<Vec<Transaction>, Box<dyn Error>>;

    /// Retrieves all transactions excluding current month and tax transactions
    fn all_transaction_excluding_current_month_and_taxes(
        &mut self,
        account_id: Uuid,
        currency: &Currency,
    ) -> Result<Vec<Transaction>, Box<dyn Error>>;

    /// Retrieves all transactions for an account in a specific currency
    fn all_transactions(
        &mut self,
        account_id: Uuid,
        currency: &Currency,
    ) -> Result<Vec<Transaction>, Box<dyn Error>>;
}

/// Trait for writing transaction data to the database
pub trait WriteTransactionDB {
    /// Creates a new transaction for an account id with the specified parameters.
    fn create_transaction_by_account_id(
        &mut self,
        account_id: Uuid,
        amount: Decimal,
        currency: &Currency,
        category: TransactionCategory,
    ) -> Result<Transaction, Box<dyn Error>>;

    /// Creates a new transaction with the specified parameters
    fn create_transaction(
        &mut self,
        account: &Account,
        amount: Decimal,
        currency: &Currency,
        category: TransactionCategory,
    ) -> Result<Transaction, Box<dyn Error>> {
        self.create_transaction_by_account_id(account.id, amount, currency, category)
    }

    /// Creates a paired transfer (withdrawal + deposit).
    /// Implementations should provide atomicity when possible.
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
        let withdrawal_tx = self.create_transaction(
            from_account,
            withdrawal_amount,
            currency,
            withdrawal_category,
        )?;
        let deposit_tx = self.create_transaction(to_account, amount, currency, deposit_category)?;
        Ok((withdrawal_tx, deposit_tx))
    }
}

/// Trait for reading execution data from the database.
pub trait ReadExecutionDB {
    /// Retrieve all executions for a trade.
    fn all_trade_executions(&mut self, trade_id: Uuid) -> Result<Vec<Execution>, Box<dyn Error>>;

    /// Retrieve all executions for an order.
    fn all_order_executions(&mut self, order_id: Uuid) -> Result<Vec<Execution>, Box<dyn Error>>;

    /// Retrieve the latest execution timestamp for a trade (if any).
    fn latest_trade_execution_at(
        &mut self,
        trade_id: Uuid,
    ) -> Result<Option<chrono::NaiveDateTime>, Box<dyn Error>>;
}

/// Trait for writing execution data to the database.
pub trait WriteExecutionDB {
    /// Insert an execution if not already present (idempotent).
    ///
    /// Dedupe is expected to be enforced by `(broker, account_id, broker_execution_id)`.
    fn upsert_execution(&mut self, execution: &Execution) -> Result<Execution, Box<dyn Error>>;
}

// Trade DB

/// Trait for reading trade data from the database
pub trait ReadTradeDB {
    /// Retrieves all open trades for a specific account and currency
    fn all_open_trades_for_currency(
        &mut self,
        account_id: Uuid,
        currency: &Currency,
    ) -> Result<Vec<Trade>, Box<dyn Error>>;

    /// Retrieves all trades with a specific status for an account
    fn read_trades_with_status(
        &mut self,
        account_id: Uuid,
        status: Status,
    ) -> Result<Vec<Trade>, Box<dyn Error>>;

    /// Retrieves a specific trade by its ID
    fn read_trade(&mut self, id: Uuid) -> Result<Trade, Box<dyn Error>>;

    /// Retrieves the status for a trade by ID.
    ///
    /// This is a lightweight alternative to `read_trade` for hot paths that only need
    /// to validate state transitions.
    fn read_trade_status(&mut self, id: Uuid) -> Result<Status, Box<dyn Error>>;

    /// Retrieves a specific trade balance by its ID
    fn read_trade_balance(&mut self, balance_id: Uuid) -> Result<TradeBalance, Box<dyn Error>>;

    /// Retrieves recent closed trade performance rows for analytics.
    ///
    /// This is a lightweight alternative to loading full `Trade` graphs when only
    /// `(updated_at, total_performance)` is required.
    fn read_recent_closed_trade_performances(
        &mut self,
        account_id: Uuid,
        currency: &Currency,
        cutoff: chrono::NaiveDateTime,
    ) -> Result<Vec<crate::ClosedTradePerformance>, Box<dyn Error>>;

    /// Retrieves recent closed trade `(updated_at, total_performance)` points for analytics caching.
    fn read_recent_closed_trade_performance_points(
        &mut self,
        account_id: Uuid,
        currency: &Currency,
        cutoff: chrono::NaiveDateTime,
    ) -> Result<Vec<(chrono::NaiveDateTime, rust_decimal::Decimal)>, Box<dyn Error>>;
}

/// Structure representing a draft trade before it's created in the database
#[derive(Debug)]
pub struct DraftTrade {
    /// The account associated with the trade
    pub account: Account,
    /// The trading vehicle (e.g., stock, ETF, bond, option) for the trade
    pub trading_vehicle: TradingVehicle,
    /// The quantity of the trading vehicle
    pub quantity: i64,
    /// The currency used for the trade
    pub currency: Currency,
    /// The category of the trade
    pub category: TradeCategory,
    /// Trade thesis - reasoning behind the trade (max 200 chars)
    pub thesis: Option<String>,
    /// Market sector (e.g., technology, healthcare, finance)
    pub sector: Option<String>,
    /// Asset class (e.g., stocks, ETFs, bonds, options, futures, crypto)
    pub asset_class: Option<String>,
    /// Trading context (e.g., Elliott Wave count, S/R levels, indicators)
    pub context: Option<String>,
}

/// Trait for writing trade data to the database
pub trait WriteTradeDB {
    /// Creates a new trade with the specified draft and orders
    fn create_trade(
        &mut self,
        draft: DraftTrade,
        stop: &Order,
        entry: &Order,
        target: &Order,
    ) -> Result<Trade, Box<dyn Error>>;

    /// Updates the status of an existing trade
    fn update_trade_status(
        &mut self,
        status: Status,
        trade: &Trade,
    ) -> Result<Trade, Box<dyn Error>>;
}

/// Trait for writing trade balance data to the database
pub trait WriteAccountBalanceDB {
    /// Updates the trade balance with performance metrics
    fn update_trade_balance(
        &mut self,
        trade: &Trade,
        funding: Decimal,
        capital_in_market: Decimal,
        capital_out_market: Decimal,
        taxed: Decimal,
        total_performance: Decimal,
    ) -> Result<TradeBalance, Box<dyn Error>>;
}

// Rule DB
/// Trait for writing rule data to the database
pub trait WriteRuleDB {
    /// Creates a new rule with the specified parameters
    fn create_rule(
        &mut self,
        account: &Account,
        name: &RuleName,
        description: &str,
        priority: u32,
        level: &RuleLevel,
    ) -> Result<Rule, Box<dyn Error>>;

    /// Marks a rule as inactive
    fn make_rule_inactive(&mut self, rule: &Rule) -> Result<Rule, Box<dyn Error>>;
}

/// Trait for reading rule data from the database
pub trait ReadRuleDB {
    /// Retrieves all rules for a specific account
    fn read_all_rules(&mut self, account_id: Uuid) -> Result<Vec<Rule>, Box<dyn Error>>;
    /// Retrieves a specific rule by account ID and rule name
    fn rule_for_account(
        &mut self,
        account_id: Uuid,
        name: &RuleName,
    ) -> Result<Rule, Box<dyn Error>>;
}

// Trading Vehicle DB
/// Trait for reading trading vehicle data from the database
pub trait ReadTradingVehicleDB {
    /// Retrieves all trading vehicles from the database
    fn read_all_trading_vehicles(&mut self) -> Result<Vec<TradingVehicle>, Box<dyn Error>>;
    /// Retrieves a specific trading vehicle by its ID
    fn read_trading_vehicle(&mut self, id: Uuid) -> Result<TradingVehicle, Box<dyn Error>>;
}

/// Trait for writing trading vehicle data to the database
pub trait WriteTradingVehicleDB {
    /// Creates a new trading vehicle with the specified parameters
    fn create_trading_vehicle(
        &mut self,
        symbol: &str,
        isin: Option<&str>,
        category: &TradingVehicleCategory,
        broker: &str,
    ) -> Result<TradingVehicle, Box<dyn Error>>;

    /// Creates or updates a trading vehicle, storing broker-provided metadata and optional enrichment.
    fn upsert_trading_vehicle(
        &mut self,
        input: TradingVehicleUpsert,
    ) -> Result<TradingVehicle, Box<dyn Error>>;
}

/// Full upsert input for trading vehicles (manual or broker-backed).
#[derive(Debug, Clone)]
pub struct TradingVehicleUpsert {
    /// Vehicle symbol as known by the broker (e.g., AAPL).
    pub symbol: String,
    /// Optional ISIN if available from enrichment/manual entry.
    pub isin: Option<String>,
    /// High-level category used by Trust (stock, ETF, bond, crypto, fiat).
    pub category: TradingVehicleCategory,
    /// Broker name used as part of the `(broker, symbol)` identity.
    pub broker: String,

    // Broker metadata
    /// Broker-native asset identifier when available.
    pub broker_asset_id: Option<String>,
    /// Exchange code reported by the broker.
    pub exchange: Option<String>,
    /// Broker-specific asset class string.
    pub broker_asset_class: Option<String>,
    /// Broker-specific lifecycle status string.
    pub broker_asset_status: Option<String>,
    /// Whether the broker marks the asset as tradable.
    pub tradable: Option<bool>,
    /// Whether margin trading is allowed for this asset.
    pub marginable: Option<bool>,
    /// Whether short selling is allowed for this asset.
    pub shortable: Option<bool>,
    /// Whether the asset is easy to borrow for shorting.
    pub easy_to_borrow: Option<bool>,
    /// Whether fractional trading is supported for this asset.
    pub fractionable: Option<bool>,

    /// Optional fixed-income terms for bonds and bond-like instruments.
    pub fixed_income: Option<crate::FixedIncomeTerms>,
}

/// Trait for writing broker log data to the database
pub trait WriteBrokerLogsDB {
    /// Creates a new log entry for a trade
    fn create_log(&mut self, log: &str, trade: &Trade) -> Result<BrokerLog, Box<dyn Error>>;
}

/// Trait for reading broker log data from the database
pub trait ReadBrokerLogsDB {
    /// Retrieves all logs associated with a specific trade
    fn read_all_logs_for_trade(&mut self, trade_id: Uuid)
        -> Result<Vec<BrokerLog>, Box<dyn Error>>;
}

/// Trait for reading post-trade mistakes from the database.
pub trait ReadMistakeDB {
    /// Read active mistakes for a trade.
    fn read_mistakes_for_trade(&mut self, trade_id: Uuid) -> Result<Vec<Mistake>, Box<dyn Error>>;

    /// Read active mistakes for an account within an inclusive creation-time period.
    fn read_mistakes_for_account_in_period(
        &mut self,
        account_id: Uuid,
        start_at: chrono::NaiveDateTime,
        end_at: chrono::NaiveDateTime,
    ) -> Result<Vec<Mistake>, Box<dyn Error>>;
}

/// Trait for writing post-trade mistakes to the database.
pub trait WriteMistakeDB {
    /// Persist a new mistake for a trade.
    fn create_mistake(&mut self, mistake: &Mistake) -> Result<Mistake, Box<dyn Error>>;
}

/// Trait for reading plan-act-review session plans from the database.
pub trait ReadSessionPlanDB {
    /// Read the active open session for an account, if one exists.
    fn read_open_session(
        &mut self,
        account_id: Uuid,
    ) -> Result<Option<SessionPlan>, Box<dyn Error>>;

    /// Read active session plans for an account within an inclusive opened-at period.
    fn read_session_plans_for_account(
        &mut self,
        account_id: Uuid,
        start_at: chrono::NaiveDateTime,
        end_at: chrono::NaiveDateTime,
    ) -> Result<Vec<SessionPlan>, Box<dyn Error>>;
}

/// Trait for writing plan-act-review session plans to the database.
pub trait WriteSessionPlanDB {
    /// Persist a new open session plan.
    fn create_session_plan(
        &mut self,
        session_plan: &SessionPlan,
    ) -> Result<SessionPlan, Box<dyn Error>>;

    /// Close an open session plan by applying review data.
    fn close_session_plan(
        &mut self,
        close: &SessionPlanClose,
    ) -> Result<SessionPlan, Box<dyn Error>>;
}

/// Trait for reading trade event catalysts from the database.
pub trait ReadTradeEventDB {
    /// Read active event catalysts for a trade.
    fn read_trade_events_for_trade(
        &mut self,
        trade_id: Uuid,
    ) -> Result<Vec<TradeEvent>, Box<dyn Error>>;
}

/// Trait for writing trade event catalysts to the database.
pub trait WriteTradeEventDB {
    /// Persist a new event catalyst for a trade.
    fn create_trade_event(&mut self, event: &TradeEvent) -> Result<TradeEvent, Box<dyn Error>>;

    /// Soft-delete a trade event by identifier.
    fn delete_trade_event(&mut self, event_id: Uuid) -> Result<(), Box<dyn Error>>;
}

/// Trait for reading trade grades from the database.
pub trait ReadTradeGradeDB {
    /// Read latest grade for a trade.
    fn read_latest_for_trade(
        &mut self,
        trade_id: Uuid,
    ) -> Result<Option<TradeGrade>, Box<dyn Error>>;

    /// Read grades for an account for the last N days (based on trade close/update time).
    fn read_for_account_days(
        &mut self,
        account_id: Uuid,
        days: u32,
    ) -> Result<Vec<TradeGrade>, Box<dyn Error>>;
}

/// Trait for writing trade grades to the database.
pub trait WriteTradeGradeDB {
    /// Persist a new grade record for a trade.
    fn create_trade_grade(&mut self, grade: &TradeGrade) -> Result<TradeGrade, Box<dyn Error>>;
}

/// Trait for reading level and level-change data.
pub trait ReadLevelDB {
    /// Retrieve current level for an account.
    fn level_for_account(&mut self, account_id: Uuid) -> Result<Level, Box<dyn Error>>;

    /// Retrieve all level change events for an account.
    fn level_changes_for_account(
        &mut self,
        account_id: Uuid,
    ) -> Result<Vec<LevelChange>, Box<dyn Error>>;

    /// Retrieve recent level change events in the last `days`.
    fn recent_level_changes(
        &mut self,
        account_id: Uuid,
        days: u32,
    ) -> Result<Vec<LevelChange>, Box<dyn Error>>;

    /// Retrieve level-adjustment policy rules for an account.
    fn level_adjustment_rules_for_account(
        &mut self,
        account_id: Uuid,
    ) -> Result<LevelAdjustmentRules, Box<dyn Error>>;
}

/// Trait for writing level and level-change data.
pub trait WriteLevelDB {
    /// Create default Level 3 profile for new account.
    fn create_default_level(&mut self, account: &Account) -> Result<Level, Box<dyn Error>>;

    /// Persist a level row update.
    fn update_level(&mut self, level: &Level) -> Result<Level, Box<dyn Error>>;

    /// Persist a level change audit event.
    fn create_level_change(
        &mut self,
        level_change: &LevelChange,
    ) -> Result<LevelChange, Box<dyn Error>>;

    /// Persist level-adjustment policy rules for an account.
    fn upsert_level_adjustment_rules(
        &mut self,
        account_id: Uuid,
        rules: &LevelAdjustmentRules,
    ) -> Result<LevelAdjustmentRules, Box<dyn Error>>;
}

/// Trait for reading distribution rules from the database.
pub trait DistributionRead {
    /// Retrieves distribution rules for a specific account.
    fn for_account(&mut self, account_id: Uuid) -> Result<DistributionRules, Box<dyn Error>>;

    /// Retrieves distribution execution history for a specific account.
    fn history_for_account(
        &mut self,
        account_id: Uuid,
    ) -> Result<Vec<DistributionHistory>, Box<dyn Error>>;
}

/// Trait for writing distribution rules to the database.
pub trait DistributionWrite {
    /// Creates or updates distribution rules for an account.
    fn create_or_update(
        &mut self,
        account_id: Uuid,
        earnings_percent: Decimal,
        tax_percent: Decimal,
        reinvestment_percent: Decimal,
        minimum_threshold: Decimal,
        configuration_password_hash: &str,
    ) -> Result<DistributionRules, Box<dyn Error>>;

    /// Persists an execution event for distribution audit/history.
    #[allow(clippy::too_many_arguments)]
    fn create_history(
        &mut self,
        source_account_id: Uuid,
        trade_id: Option<Uuid>,
        original_amount: Decimal,
        distribution_date: chrono::NaiveDateTime,
        earnings_amount: Option<Decimal>,
        tax_amount: Option<Decimal>,
        reinvestment_amount: Option<Decimal>,
    ) -> Result<DistributionHistory, Box<dyn Error>>;

    /// Executes all distribution transfers and writes a history row atomically.
    ///
    /// Returns destination-side transaction IDs (deposits) created for this distribution.
    fn execute_distribution_plan_atomic(
        &mut self,
        plan: &DistributionExecutionPlan,
    ) -> Result<Vec<Uuid>, Box<dyn Error>>;
}

/// Alias for advisory concentration thresholds:
/// sector, asset class, and single position percentages.
pub type AdvisoryThresholds = (Decimal, Decimal, Decimal);

/// Trait for reading advisory threshold configuration.
pub trait AdvisoryRead {
    /// Loads persisted advisory concentration thresholds for a single account.
    ///
    /// Returns:
    /// - `Ok(Some(...))` when advisory thresholds are stored for the account.
    /// - `Ok(None)` when no thresholds are configured.
    fn advisory_thresholds_for_account(
        &mut self,
        account_id: Uuid,
    ) -> Result<Option<AdvisoryThresholds>, Box<dyn Error>>;
}

/// Trait for writing advisory threshold configuration
pub trait AdvisoryWrite {
    /// Persists advisory concentration thresholds for a single account.
    ///
    /// Implementations should replace existing rows when the account already
    /// has configured thresholds.
    fn upsert_advisory_thresholds(
        &mut self,
        account_id: Uuid,
        sector_limit_pct: Decimal,
        asset_class_limit_pct: Decimal,
        single_position_limit_pct: Decimal,
    ) -> Result<(), Box<dyn Error>>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use rust_decimal_macros::dec;

    #[derive(Default)]
    struct RecordingAccountWrite {
        calls: Vec<(String, String, Environment, Decimal, Decimal)>,
    }

    impl AccountWrite for RecordingAccountWrite {
        fn create(
            &mut self,
            name: &str,
            description: &str,
            environment: Environment,
            taxes_percentage: Decimal,
            earnings_percentage: Decimal,
        ) -> Result<Account, Box<dyn Error>> {
            self.calls.push((
                name.to_string(),
                description.to_string(),
                environment,
                taxes_percentage,
                earnings_percentage,
            ));

            Ok(Account {
                name: name.to_string(),
                description: description.to_string(),
                environment,
                taxes_percentage,
                earnings_percentage,
                ..Account::default()
            })
        }
    }

    #[derive(Default)]
    struct RecordingTransactionWrite {
        writes: Vec<Transaction>,
    }

    impl WriteTransactionDB for RecordingTransactionWrite {
        fn create_transaction_by_account_id(
            &mut self,
            account_id: Uuid,
            amount: Decimal,
            currency: &Currency,
            category: TransactionCategory,
        ) -> Result<Transaction, Box<dyn Error>> {
            let now = Utc::now().naive_utc();
            let transaction = Transaction {
                id: Uuid::new_v4(),
                created_at: now,
                updated_at: now,
                deleted_at: None,
                category,
                currency: *currency,
                amount,
                account_id,
            };
            self.writes.push(transaction.clone());
            Ok(transaction)
        }
    }

    struct FailingTransactionWrite {
        calls: usize,
        fail_on_call: usize,
    }

    impl WriteTransactionDB for FailingTransactionWrite {
        fn create_transaction_by_account_id(
            &mut self,
            account_id: Uuid,
            amount: Decimal,
            currency: &Currency,
            category: TransactionCategory,
        ) -> Result<Transaction, Box<dyn Error>> {
            self.calls = self.calls.saturating_add(1);
            if self.calls == self.fail_on_call {
                return Err("forced transaction failure".into());
            }

            let now = Utc::now().naive_utc();
            Ok(Transaction {
                id: Uuid::new_v4(),
                created_at: now,
                updated_at: now,
                deleted_at: None,
                category,
                currency: *currency,
                amount,
                account_id,
            })
        }
    }

    #[test]
    fn account_write_default_hierarchy_and_profile_delegate_to_create() {
        let mut writer = RecordingAccountWrite::default();
        let parent_id = Uuid::new_v4();

        let hierarchy = writer
            .create_with_hierarchy(
                "fallback-hierarchy",
                "delegates",
                Environment::Live,
                dec!(10),
                dec!(20),
                AccountType::Reinvestment,
                Some(parent_id),
            )
            .expect("hierarchy fallback should create account");
        let profile = writer
            .create_with_profile(
                "fallback-profile",
                "delegates",
                Environment::Paper,
                dec!(5),
                dec!(15),
                AccountType::Earnings,
                Some(parent_id),
                BrokerKind::Alpaca,
                Some("broker-account"),
            )
            .expect("profile fallback should create account");

        assert_eq!(hierarchy.name, "fallback-hierarchy");
        assert_eq!(profile.name, "fallback-profile");
        assert_eq!(writer.calls.len(), 2);
        assert_eq!(
            writer.calls.first().expect("first create call").0,
            "fallback-hierarchy"
        );
        assert_eq!(
            writer.calls.get(1).expect("second create call").0,
            "fallback-profile"
        );
    }

    #[test]
    fn transaction_write_defaults_create_account_transactions_and_transfer_pairs() {
        let mut writer = RecordingTransactionWrite::default();
        let source = Account::default();
        let destination = Account::default();

        let deposit = writer
            .create_transaction(
                &source,
                dec!(100),
                &Currency::USD,
                TransactionCategory::Deposit,
            )
            .expect("default create_transaction should delegate by account id");
        let (withdrawal, transfer_deposit) = writer
            .create_transfer_pair(
                &source,
                &destination,
                dec!(25),
                &Currency::USD,
                TransactionCategory::Withdrawal,
                TransactionCategory::Deposit,
            )
            .expect("default transfer pair should create both legs");

        assert_eq!(deposit.account_id, source.id);
        assert_eq!(withdrawal.account_id, source.id);
        assert_eq!(withdrawal.amount, dec!(-25));
        assert_eq!(transfer_deposit.account_id, destination.id);
        assert_eq!(transfer_deposit.amount, dec!(25));
        assert_eq!(writer.writes.len(), 3);
    }

    #[test]
    fn transaction_write_transfer_pair_returns_first_or_second_leg_failures() {
        let source = Account::default();
        let destination = Account::default();

        let mut first_leg_fails = FailingTransactionWrite {
            calls: 0,
            fail_on_call: 1,
        };
        let first_error = first_leg_fails
            .create_transfer_pair(
                &source,
                &destination,
                dec!(25),
                &Currency::USD,
                TransactionCategory::Withdrawal,
                TransactionCategory::Deposit,
            )
            .expect_err("withdrawal leg failure should return an error");
        assert!(first_error
            .to_string()
            .contains("forced transaction failure"));
        assert_eq!(first_leg_fails.calls, 1);

        let mut second_leg_fails = FailingTransactionWrite {
            calls: 0,
            fail_on_call: 2,
        };
        let second_error = second_leg_fails
            .create_transfer_pair(
                &source,
                &destination,
                dec!(25),
                &Currency::USD,
                TransactionCategory::Withdrawal,
                TransactionCategory::Deposit,
            )
            .expect_err("deposit leg failure should return an error");
        assert!(second_error
            .to_string()
            .contains("forced transaction failure"));
        assert_eq!(second_leg_fails.calls, 2);
    }
}
