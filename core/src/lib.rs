//! Trust Core Crate - Business Logic and Risk Management
//!
//! This crate contains the core business logic, calculators, and validators
//! for the Trust financial trading application.

// === FINANCIAL APPLICATION SAFETY LINTS ===
// These lint rules are critical for financial applications where precision,
// safety, and reliability are paramount. Violations can lead to financial losses.

#![deny(
    // Error handling safety - force proper error handling
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic,

    // Financial precision safety - prevent calculation errors
    clippy::float_arithmetic,
    clippy::arithmetic_side_effects,
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,

    // Code quality enforcement
    clippy::cognitive_complexity,
    clippy::too_many_lines,
)]
// Allow unwrap and expect in test code only
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]
// Standard Rust lints for code quality
#![warn(missing_docs, rust_2018_idioms, missing_debug_implementations)]

use crate::services::{EventDistributionService, FundTransferService, ProfitDistributionService};
use argon2::{
    password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString},
    Argon2,
};
use calculators_trade::{LevelAdjustedQuantity, QuantityCalculator};
use events::trade::{CloseReason, TradeClosed};
use model::database::TradingVehicleUpsert;
use model::{
    Account, AccountBalance, AccountType, Broker, BrokerLog, Currency, DatabaseFactory,
    DistributionHistory, DistributionResult, DistributionRules, DraftTrade, Environment, Execution,
    Level, LevelAdjustmentRules, LevelChange, LevelTrigger, Order, Rule, RuleLevel, RuleName,
    Status, Trade, TradeBalance, TradingVehicle, TradingVehicleCategory, Transaction,
    TransactionCategory,
};
use rand_core::OsRng;
use rust_decimal::Decimal;
use sha2::{Digest, Sha256};
use uuid::Uuid;
use {
    services::leveling::{
        DefaultLevelTransitionPolicy, LevelEvaluationOutcome, LevelPerformanceSnapshot,
        LevelingService,
    },
    std::error::Error as StdError,
};

/// Summary data combining all key trading metrics
#[derive(Debug, Clone)]
pub struct TradingSummary {
    /// Account ID this summary is for
    pub account_id: Option<Uuid>,
    /// Account equity/balance
    pub equity: Decimal,
    /// Performance metrics (if available)
    pub performance: Option<calculators_performance::PerformanceStats>,
    /// Capital at risk data
    pub capital_at_risk: Vec<calculators_risk::OpenPosition>,
    /// Concentration data
    pub concentration: Vec<calculators_concentration::ConcentrationGroup>,
}

/// The main facade for interacting with the Trust financial trading system.
///
/// This struct provides a unified interface for all core operations including
/// account management, trade execution, risk management, and transaction handling.
/// It encapsulates the database factory and broker implementations.
pub struct TrustFacade {
    factory: Box<dyn DatabaseFactory>,
    broker: Box<dyn Broker>,
    protected_mode: bool,
    protected_authorized: bool,
}

impl std::fmt::Debug for TrustFacade {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TrustFacade")
            .field("factory", &"Box<dyn DatabaseFactory>")
            .field("broker", &"Box<dyn Broker>")
            .field("protected_mode", &self.protected_mode)
            .finish()
    }
}

/// Trust is the main entry point for interacting with the core library.
/// It is a facade that provides a simple interface for interacting with the
/// core library.
impl TrustFacade {
    /// Creates a new instance of Trust.
    pub fn new(factory: Box<dyn DatabaseFactory>, broker: Box<dyn Broker>) -> Self {
        TrustFacade {
            factory,
            broker,
            protected_mode: false,
            protected_authorized: false,
        }
    }

    /// Enables protected-mutation enforcement for this facade instance.
    pub fn enable_protected_mode(&mut self) {
        self.protected_mode = true;
    }

    /// Authorizes exactly one protected mutation operation.
    pub fn authorize_protected_mutation(&mut self) {
        self.protected_authorized = true;
    }

    fn consume_protected_authorization(
        &mut self,
        operation: &'static str,
    ) -> Result<(), Box<dyn std::error::Error>> {
        if !self.protected_mode {
            return Ok(());
        }
        if !self.protected_authorized {
            return Err(format!("Protected mutation '{operation}' requires authorization").into());
        }
        self.protected_authorized = false;
        Ok(())
    }

    /// Creates a new account.
    pub fn create_account(
        &mut self,
        name: &str,
        description: &str,
        environment: Environment,
        taxes_percentage: Decimal,
        earnings_percentage: Decimal,
    ) -> Result<Account, Box<dyn std::error::Error>> {
        self.consume_protected_authorization("create_account")?;
        let savepoint = "create_account_with_level";
        self.factory.begin_savepoint(savepoint)?;

        let account = match self.factory.account_write().create(
            name,
            description,
            environment,
            taxes_percentage,
            earnings_percentage,
        ) {
            Ok(account) => account,
            Err(error) => {
                let _ = self.factory.rollback_to_savepoint(savepoint);
                return Err(error);
            }
        };

        if let Err(error) = self.factory.level_write().create_default_level(&account) {
            let _ = self.factory.rollback_to_savepoint(savepoint);
            return Err(error);
        }

        self.factory.release_savepoint(savepoint)?;

        Ok(account)
    }

    /// Returns current level for an account.
    pub fn level_for_account(&mut self, account_id: Uuid) -> Result<Level, Box<dyn StdError>> {
        self.factory.level_read().level_for_account(account_id)
    }

    /// Returns account level change history. If `days` is provided, applies a recent-window filter.
    pub fn level_history_for_account(
        &mut self,
        account_id: Uuid,
        days: Option<u32>,
    ) -> Result<Vec<LevelChange>, Box<dyn StdError>> {
        if let Some(window_days) = days {
            return self
                .factory
                .level_read()
                .recent_level_changes(account_id, window_days);
        }
        self.factory
            .level_read()
            .level_changes_for_account(account_id)
    }

    /// Changes account level and records an immutable audit event atomically.
    pub fn change_level(
        &mut self,
        account_id: Uuid,
        target_level: u8,
        reason: &str,
        trigger_type: LevelTrigger,
    ) -> Result<(Level, LevelChange), Box<dyn StdError>> {
        self.consume_protected_authorization("change_level")?;
        commands::level::change(
            &mut *self.factory,
            account_id,
            target_level,
            reason,
            trigger_type,
        )
    }

    /// Evaluates transition policy and optionally applies it.
    pub fn evaluate_level_transition(
        &mut self,
        account_id: Uuid,
        snapshot: LevelPerformanceSnapshot,
        apply: bool,
    ) -> Result<LevelEvaluationOutcome, Box<dyn StdError>> {
        if apply {
            self.consume_protected_authorization("evaluate_level_transition_apply")?;
        }
        let service = self.leveling_service_for_account(account_id)?;
        service.evaluate_and_apply(&mut *self.factory, account_id, &snapshot, apply)
    }

    /// Retrieve level-adjustment policy rules for an account.
    pub fn level_adjustment_rules_for_account(
        &mut self,
        account_id: Uuid,
    ) -> Result<LevelAdjustmentRules, Box<dyn StdError>> {
        self.factory
            .level_read()
            .level_adjustment_rules_for_account(account_id)
    }

    /// Persist level-adjustment policy rules for an account.
    pub fn set_level_adjustment_rules(
        &mut self,
        account_id: Uuid,
        rules: &LevelAdjustmentRules,
    ) -> Result<LevelAdjustmentRules, Box<dyn StdError>> {
        self.consume_protected_authorization("set_level_adjustment_rules")?;
        rules.validate()?;
        self.factory
            .level_write()
            .upsert_level_adjustment_rules(account_id, rules)
    }

    /// Search for an account by name.
    ///
    /// # Arguments
    ///
    /// * `name` - The name of the account to search for
    ///
    /// # Returns
    ///
    /// Returns the account if found, or an error if not found.
    pub fn search_account(&mut self, name: &str) -> Result<Account, Box<dyn std::error::Error>> {
        self.factory.account_read().for_name(name)
    }

    /// Retrieve all accounts in the system.
    ///
    /// # Returns
    ///
    /// Returns a vector of all accounts, or an error if the operation fails.
    pub fn search_all_accounts(&mut self) -> Result<Vec<Account>, Box<dyn std::error::Error>> {
        self.factory.account_read().all()
    }

    /// Retrieve all risk management rules for a specific account.
    ///
    /// # Arguments
    ///
    /// * `account_id` - The UUID of the account to retrieve rules for
    ///
    /// # Returns
    ///
    /// Returns a vector of all rules for the account, or an error if the operation fails.
    pub fn search_all_rules(
        &mut self,
        account_id: Uuid,
    ) -> Result<Vec<Rule>, Box<dyn std::error::Error>> {
        self.factory.rule_read().read_all_rules(account_id)
    }

    /// Create a new financial transaction for an account.
    ///
    /// # Arguments
    ///
    /// * `account` - The account to create the transaction for
    /// * `category` - The category of the transaction (deposit, withdrawal, etc.)
    /// * `amount` - The amount of the transaction
    /// * `currency` - The currency of the transaction
    ///
    /// # Returns
    ///
    /// Returns a tuple of the created transaction and updated account balance.
    pub fn create_transaction(
        &mut self,
        account: &Account,
        category: &TransactionCategory,
        amount: Decimal,
        currency: &Currency,
    ) -> Result<(Transaction, AccountBalance), Box<dyn std::error::Error>> {
        self.consume_protected_authorization("create_transaction")?;
        commands::transaction::create(&mut *self.factory, category, amount, currency, account.id)
    }

    /// Search for the account balance in a specific currency.
    ///
    /// # Arguments
    ///
    /// * `account_id` - The UUID of the account
    /// * `currency` - The currency to get the balance for
    ///
    /// # Returns
    ///
    /// Returns the account balance for the specified currency.
    pub fn search_balance(
        &mut self,
        account_id: Uuid,
        currency: &Currency,
    ) -> Result<AccountBalance, Box<dyn std::error::Error>> {
        self.factory
            .account_balance_read()
            .for_currency(account_id, currency)
    }

    /// Retrieve all account balances across all currencies.
    ///
    /// # Arguments
    ///
    /// * `account_id` - The UUID of the account
    ///
    /// # Returns
    ///
    /// Returns a vector of all account balances for all currencies.
    pub fn search_all_balances(
        &mut self,
        account_id: Uuid,
    ) -> Result<Vec<AccountBalance>, Box<dyn std::error::Error>> {
        self.factory.account_balance_read().for_account(account_id)
    }

    /// Create a new risk management rule for an account.
    ///
    /// # Arguments
    ///
    /// * `account` - The account to create the rule for
    /// * `name` - The name/type of the rule (e.g., RiskPerTrade, RiskPerMonth)
    /// * `description` - A description of the rule
    /// * `level` - The priority level of the rule
    ///
    /// # Returns
    ///
    /// Returns the created rule, or an error if creation fails.
    pub fn create_rule(
        &mut self,
        account: &Account,
        name: &RuleName,
        description: &str,
        level: &RuleLevel,
    ) -> Result<Rule, Box<dyn std::error::Error>> {
        self.consume_protected_authorization("create_rule")?;
        commands::rule::create(&mut *self.factory, account, name, description, level)
    }

    /// Deactivate an existing risk management rule.
    ///
    /// # Arguments
    ///
    /// * `rule` - The rule to deactivate
    ///
    /// # Returns
    ///
    /// Returns the deactivated rule, or an error if deactivation fails.
    pub fn deactivate_rule(&mut self, rule: &Rule) -> Result<Rule, Box<dyn std::error::Error>> {
        self.consume_protected_authorization("deactivate_rule")?;
        self.factory.rule_write().make_rule_inactive(rule)
    }

    /// Search for all active rules for a specific account.
    ///
    /// # Arguments
    ///
    /// * `account_id` - The UUID of the account
    ///
    /// # Returns
    ///
    /// Returns a vector of active rules for the account.
    pub fn search_rules(
        &mut self,
        account_id: Uuid,
    ) -> Result<Vec<Rule>, Box<dyn std::error::Error>> {
        self.factory.rule_read().read_all_rules(account_id)
    }

    /// Create a new trading vehicle (stock, ETF, etc.).
    ///
    /// # Arguments
    ///
    /// * `symbol` - The trading symbol (e.g., "AAPL")
    /// * `isin` - The International Securities Identification Number
    /// * `category` - The category of the trading vehicle
    /// * `broker` - The broker name
    ///
    /// # Returns
    ///
    /// Returns the created trading vehicle.
    pub fn create_trading_vehicle(
        &mut self,
        symbol: &str,
        isin: Option<&str>,
        category: &TradingVehicleCategory,
        broker: &str,
    ) -> Result<TradingVehicle, Box<dyn std::error::Error>> {
        self.consume_protected_authorization("create_trading_vehicle")?;
        self.factory
            .trading_vehicle_write()
            .create_trading_vehicle(symbol, isin, category, broker)
    }

    /// Create or update a trading vehicle, storing broker metadata and optional enrichment.
    pub fn upsert_trading_vehicle(
        &mut self,
        input: TradingVehicleUpsert,
    ) -> Result<TradingVehicle, Box<dyn std::error::Error>> {
        self.consume_protected_authorization("upsert_trading_vehicle")?;
        self.factory
            .trading_vehicle_write()
            .upsert_trading_vehicle(input)
    }

    /// Retrieve all available trading vehicles.
    ///
    /// # Returns
    ///
    /// Returns a vector of all trading vehicles in the system.
    pub fn search_trading_vehicles(
        &mut self,
    ) -> Result<Vec<TradingVehicle>, Box<dyn std::error::Error>> {
        self.factory
            .trading_vehicle_read()
            .read_all_trading_vehicles()
    }

    /// Calculate the maximum quantity that can be traded based on risk rules.
    ///
    /// # Arguments
    ///
    /// * `account_id` - The UUID of the account
    /// * `entry_price` - The planned entry price
    /// * `stop_price` - The stop loss price
    /// * `currency` - The currency of the trade
    ///
    /// # Returns
    ///
    /// Returns the maximum quantity allowed by risk management rules.
    pub fn calculate_maximum_quantity(
        &mut self,
        account_id: Uuid,
        entry_price: Decimal,
        stop_price: Decimal,
        currency: &Currency,
    ) -> Result<i64, Box<dyn std::error::Error>> {
        let adjusted = QuantityCalculator::maximum_quantity_with_level(
            account_id,
            entry_price,
            stop_price,
            currency,
            &mut *self.factory,
        )?;
        Ok(adjusted.final_quantity)
    }

    /// Calculate base and level-adjusted maximum quantity for visibility and validation.
    pub fn calculate_level_adjusted_quantity(
        &mut self,
        account_id: Uuid,
        entry_price: Decimal,
        stop_price: Decimal,
        currency: &Currency,
    ) -> Result<LevelAdjustedQuantity, Box<dyn std::error::Error>> {
        QuantityCalculator::maximum_quantity_with_level(
            account_id,
            entry_price,
            stop_price,
            currency,
            &mut *self.factory,
        )
    }

    /// Create a new trade with entry, stop, and target orders.
    ///
    /// # Arguments
    ///
    /// * `trade` - The draft trade information
    /// * `stop_price` - The stop loss price
    /// * `entry_price` - The entry price
    /// * `target_price` - The target (take profit) price
    ///
    /// # Returns
    ///
    /// Returns the created trade with all associated orders.
    pub fn create_trade(
        &mut self,
        trade: DraftTrade,
        stop_price: Decimal,
        entry_price: Decimal,
        target_price: Decimal,
    ) -> Result<Trade, Box<dyn std::error::Error>> {
        commands::trade::create_trade(
            trade,
            stop_price,
            entry_price,
            target_price,
            &mut *self.factory,
        )
    }

    /// Search for trades by account and status.
    ///
    /// # Arguments
    ///
    /// * `account_id` - The UUID of the account
    /// * `status` - The status to filter trades by
    ///
    /// # Returns
    ///
    /// Returns a vector of trades matching the criteria.
    pub fn search_trades(
        &mut self,
        account_id: Uuid,
        status: Status,
    ) -> Result<Vec<Trade>, Box<dyn std::error::Error>> {
        self.factory
            .trade_read()
            .read_trades_with_status(account_id, status)
    }

    /// Retrieve all executions (fills) attributed to a trade.
    pub fn executions_for_trade(
        &mut self,
        trade_id: Uuid,
    ) -> Result<Vec<Execution>, Box<dyn std::error::Error>> {
        self.factory.execution_read().all_trade_executions(trade_id)
    }

    /// Get all transactions for a specific account
    ///
    /// # Arguments
    /// * `account_id` - The account ID to get transactions for
    ///
    /// # Returns
    /// Returns all transactions for the account, excluding taxes
    pub fn get_account_transactions(
        &mut self,
        account_id: Uuid,
    ) -> Result<Vec<Transaction>, Box<dyn std::error::Error>> {
        // Use USD as default currency for now
        self.factory
            .transaction_read()
            .all_account_transactions_excluding_taxes(account_id, &Currency::USD)
    }

    /// Get all transactions across all accounts
    ///
    /// # Returns
    /// Returns all transactions for all accounts
    pub fn get_all_transactions(&mut self) -> Result<Vec<Transaction>, Box<dyn std::error::Error>> {
        let accounts = self.search_all_accounts()?;
        let mut all_transactions = Vec::new();

        for account in accounts {
            if let Ok(txns) = self.get_account_transactions(account.id) {
                all_transactions.extend(txns);
            }
        }

        Ok(all_transactions)
    }

    /// Search for all closed trades (both target and stop loss) for an account.
    ///
    /// # Arguments
    ///
    /// * `account_id` - The UUID of the account, or None to get all accounts
    ///
    /// # Returns
    ///
    /// Returns a vector of all closed trades (ClosedTarget and ClosedStopLoss).
    pub fn search_closed_trades(
        &mut self,
        account_id: Option<Uuid>,
    ) -> Result<Vec<Trade>, Box<dyn std::error::Error>> {
        let mut all_trades = Vec::new();

        if let Some(id) = account_id {
            // Get trades for specific account
            if let Ok(mut trades) = self.search_trades(id, Status::ClosedTarget) {
                all_trades.append(&mut trades);
            }
            if let Ok(mut trades) = self.search_trades(id, Status::ClosedStopLoss) {
                all_trades.append(&mut trades);
            }
        } else {
            // Get all accounts first, then get their trades
            let accounts = self.search_all_accounts()?;
            for account in accounts {
                if let Ok(mut trades) = self.search_trades(account.id, Status::ClosedTarget) {
                    all_trades.append(&mut trades);
                }
                if let Ok(mut trades) = self.search_trades(account.id, Status::ClosedStopLoss) {
                    all_trades.append(&mut trades);
                }
            }
        }

        Ok(all_trades)
    }

    // Trade Steps

    /// Fund a trade by transferring capital from the account.
    ///
    /// # Arguments
    ///
    /// * `trade` - The trade to fund
    ///
    /// # Returns
    ///
    /// Returns a tuple of the updated trade, transaction, account balance, and trade balance.
    pub fn fund_trade(
        &mut self,
        trade: &Trade,
    ) -> Result<(Trade, Transaction, AccountBalance, TradeBalance), Box<dyn std::error::Error>>
    {
        commands::trade::fund(trade, &mut *self.factory)
    }

    /// Submit a funded trade to the broker for execution.
    ///
    /// # Arguments
    ///
    /// * `trade` - The funded trade to submit
    ///
    /// # Returns
    ///
    /// Returns a tuple of the submitted trade and broker log.
    pub fn submit_trade(
        &mut self,
        trade: &Trade,
    ) -> Result<(Trade, BrokerLog), Box<dyn std::error::Error>> {
        commands::trade::submit(trade, &mut *self.factory, &mut *self.broker)
    }

    /// Synchronize trade status with the broker.
    ///
    /// # Arguments
    ///
    /// * `trade` - The trade to synchronize
    /// * `account` - The account associated with the trade
    ///
    /// # Returns
    ///
    /// Returns a tuple of the updated status, orders, and broker log.
    pub fn sync_trade(
        &mut self,
        trade: &Trade,
        account: &Account,
    ) -> Result<(Status, Vec<Order>, BrokerLog), Box<dyn std::error::Error>> {
        let (status, orders, log) = commands::trade::sync_with_broker(
            trade,
            account,
            &mut *self.factory,
            &mut *self.broker,
        )?;

        // Best-effort auto-grade when the trade closes. Never fail the sync because grading
        // depends on optional broker market data.
        if status == Status::ClosedTarget || status == Status::ClosedStopLoss {
            let has_grade = match self
                .factory
                .trade_grade_read()
                .read_latest_for_trade(trade.id)
            {
                Ok(opt) => opt.is_some(),
                Err(_) => true, // Can't read grades; treat as "don't try" to keep sync reliable.
            };

            if !has_grade {
                let mut grader = crate::services::grading::TradeGradeService::new(
                    &mut *self.factory,
                    &mut *self.broker,
                );
                let _ = grader.grade_trade(
                    trade.id,
                    crate::services::grading::GradingWeightsPermille::default(),
                );
            }

            let close_reason = if status == Status::ClosedTarget {
                CloseReason::Target
            } else {
                CloseReason::StopLoss
            };
            let _ = self.handle_trade_closed_event(trade.id, close_reason);
        }

        Ok((status, orders, log))
    }

    /// Grade a closed trade and persist its grade.
    pub fn grade_trade(
        &mut self,
        trade_id: Uuid,
        weights: crate::services::grading::GradingWeightsPermille,
    ) -> Result<crate::services::grading::DetailedTradeGrade, Box<dyn std::error::Error>> {
        let mut grader =
            crate::services::grading::TradeGradeService::new(&mut *self.factory, &mut *self.broker);
        grader.grade_trade(trade_id, weights)
    }

    /// Compute a trade grade (without persisting it).
    pub fn compute_trade_grade(
        &mut self,
        trade_id: Uuid,
        weights: crate::services::grading::GradingWeightsPermille,
    ) -> Result<crate::services::grading::DetailedTradeGrade, Box<dyn std::error::Error>> {
        let mut grader =
            crate::services::grading::TradeGradeService::new(&mut *self.factory, &mut *self.broker);
        grader.compute_grade(trade_id, weights)
    }

    /// Retrieve the latest grade for a trade (if any).
    pub fn latest_trade_grade(
        &mut self,
        trade_id: Uuid,
    ) -> Result<Option<model::TradeGrade>, Box<dyn std::error::Error>> {
        self.factory
            .trade_grade_read()
            .read_latest_for_trade(trade_id)
    }

    /// Retrieve grades for an account over the last N days.
    pub fn trade_grades_for_account_days(
        &mut self,
        account_id: Uuid,
        days: u32,
    ) -> Result<Vec<model::TradeGrade>, Box<dyn std::error::Error>> {
        self.factory
            .trade_grade_read()
            .read_for_account_days(account_id, days)
    }

    /// Mark a trade as filled and create the appropriate transactions.
    ///
    /// # Arguments
    ///
    /// * `trade` - The trade that was filled
    /// * `fee` - The broker fee for the trade
    ///
    /// # Returns
    ///
    /// Returns a tuple of the updated trade and transaction.
    pub fn fill_trade(
        &mut self,
        trade: &Trade,
        fee: Decimal,
    ) -> Result<(Trade, Transaction), Box<dyn std::error::Error>> {
        commands::trade::fill_trade(trade, fee, self.factory.as_mut())
    }

    /// Handle a trade that hit its stop loss.
    ///
    /// # Arguments
    ///
    /// * `trade` - The trade that hit stop loss
    /// * `fee` - The broker fee for the trade
    ///
    /// # Returns
    ///
    /// Returns a tuple of transactions, trade balance, and account balance.
    pub fn stop_trade(
        &mut self,
        trade: &Trade,
        fee: Decimal,
    ) -> Result<(Transaction, Transaction, TradeBalance, AccountBalance), Box<dyn std::error::Error>>
    {
        let result = commands::trade::stop_acquired(trade, fee, &mut *self.factory)?;
        let _ = self.handle_trade_closed_event(trade.id, CloseReason::StopLoss);
        Ok(result)
    }

    /// Close an open trade at market price.
    ///
    /// # Arguments
    ///
    /// * `trade` - The trade to close
    ///
    /// # Returns
    ///
    /// Returns a tuple of the trade balance and broker log.
    pub fn close_trade(
        &mut self,
        trade: &Trade,
    ) -> Result<(TradeBalance, BrokerLog), Box<dyn std::error::Error>> {
        let result = commands::trade::close(trade, &mut *self.factory, &mut *self.broker)?;
        let _ = self.handle_trade_closed_event(trade.id, CloseReason::Manual);
        Ok(result)
    }

    /// Close a trade with automatic profit distribution
    pub fn close_trade_with_auto_distribution(
        &mut self,
        trade: &Trade,
    ) -> Result<(TradeBalance, BrokerLog, Option<DistributionResult>), Box<dyn std::error::Error>>
    {
        // 1. Close the trade normally
        let (balance, log) = self.close_trade(trade)?;

        // 2. Trigger automatic distribution if trade was profitable
        let mut event_service = EventDistributionService::new(&mut *self.factory);
        let distribution_result =
            event_service.handle_trade_closed_event(trade, &trade.currency)?;

        Ok((balance, log, distribution_result))
    }

    /// Cancel a funded trade and return capital to the account.
    ///
    /// # Arguments
    ///
    /// * `trade` - The funded trade to cancel
    ///
    /// # Returns
    ///
    /// Returns a tuple of trade balance, account balance, and transaction.
    pub fn cancel_funded_trade(
        &mut self,
        trade: &Trade,
    ) -> Result<(TradeBalance, AccountBalance, Transaction), Box<dyn std::error::Error>> {
        commands::trade::cancel_funded(trade, &mut *self.factory)
    }

    /// Cancel a submitted trade with the broker.
    ///
    /// # Arguments
    ///
    /// * `trade` - The submitted trade to cancel
    ///
    /// # Returns
    ///
    /// Returns a tuple of trade balance, account balance, and transaction.
    pub fn cancel_submitted_trade(
        &mut self,
        trade: &Trade,
    ) -> Result<(TradeBalance, AccountBalance, Transaction), Box<dyn std::error::Error>> {
        commands::trade::cancel_submitted(trade, &mut *self.factory, &mut *self.broker)
    }

    /// Handle a trade that reached its target price.
    ///
    /// # Arguments
    ///
    /// * `trade` - The trade that hit target
    /// * `fee` - The broker fee for the trade
    ///
    /// # Returns
    ///
    /// Returns a tuple of transactions, trade balance, and account balance.
    pub fn target_acquired(
        &mut self,
        trade: &Trade,
        fee: Decimal,
    ) -> Result<(Transaction, Transaction, TradeBalance, AccountBalance), Box<dyn std::error::Error>>
    {
        let result = commands::trade::target_acquired(trade, fee, &mut *self.factory)?;
        let _ = self.handle_trade_closed_event(trade.id, CloseReason::Target);
        Ok(result)
    }

    fn handle_trade_closed_event(
        &mut self,
        trade_id: Uuid,
        close_reason: CloseReason,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let trade = self.factory.trade_read().read_trade(trade_id)?;
        let risk_per_share = match trade
            .entry
            .unit_price
            .checked_sub(trade.safety_stop.unit_price)
        {
            Some(value) if value > Decimal::ZERO => value,
            _ => Decimal::ZERO,
        };
        let qty = Decimal::from(trade.entry.quantity);
        let risk_amount = risk_per_share.checked_mul(qty).unwrap_or(Decimal::ZERO);
        let r_multiple = if risk_amount > Decimal::ZERO {
            trade
                .balance
                .total_performance
                .checked_div(risk_amount)
                .unwrap_or(Decimal::ZERO)
        } else {
            Decimal::ZERO
        };

        let event = TradeClosed {
            trade_id: trade.id,
            account_id: trade.account_id,
            final_pnl: trade.balance.total_performance,
            r_multiple,
            close_reason,
            closed_at: trade.updated_at,
        };

        let service = self.leveling_service_for_account(trade.account_id)?;
        let _ = service.handle_trade_closed(&mut *self.factory, &event)?;
        Ok(())
    }

    fn leveling_service_for_account(
        &mut self,
        account_id: Uuid,
    ) -> Result<LevelingService<DefaultLevelTransitionPolicy>, Box<dyn std::error::Error>> {
        let rules = self
            .factory
            .level_read()
            .level_adjustment_rules_for_account(account_id)?;
        let policy = DefaultLevelTransitionPolicy::new(rules.clone());
        Ok(LevelingService::new(policy).with_stabilization_rules(
            rules.min_trades_at_level_for_upgrade,
            rules.max_changes_in_30_days,
        ))
    }

    /// Modify the stop loss price of an active trade.
    ///
    /// # Arguments
    ///
    /// * `trade` - The trade to modify
    /// * `account` - The account associated with the trade
    /// * `new_stop_price` - The new stop loss price
    ///
    /// # Returns
    ///
    /// Returns the updated trade.
    pub fn modify_stop(
        &mut self,
        trade: &Trade,
        account: &Account,
        new_stop_price: Decimal,
    ) -> Result<Trade, Box<dyn std::error::Error>> {
        commands::trade::modify_stop(
            trade,
            account,
            new_stop_price,
            &mut *self.broker,
            &mut *self.factory,
        )
    }

    /// Modify the target price of an active trade.
    ///
    /// # Arguments
    ///
    /// * `trade` - The trade to modify
    /// * `account` - The account associated with the trade
    /// * `new_target_price` - The new target price
    ///
    /// # Returns
    ///
    /// Returns the updated trade.
    pub fn modify_target(
        &mut self,
        trade: &Trade,
        account: &Account,
        new_target_price: Decimal,
    ) -> Result<Trade, Box<dyn std::error::Error>> {
        commands::trade::modify_target(
            trade,
            account,
            new_target_price,
            &mut *self.broker,
            &mut *self.factory,
        )
    }

    /// Calculate open positions for capital at risk reporting
    ///
    /// # Arguments
    /// * `account_id` - Optional account ID to filter by
    ///
    /// # Returns
    /// Returns a vector of open positions with their capital at risk
    pub fn calculate_open_positions(
        &mut self,
        account_id: Option<Uuid>,
    ) -> Result<Vec<calculators_risk::OpenPosition>, Box<dyn std::error::Error>> {
        calculators_risk::CapitalAtRiskCalculator::calculate_open_positions(
            account_id,
            &mut *self.factory,
        )
    }

    /// Calculate portfolio concentration by asset category
    ///
    /// # Arguments
    /// * `account_id` - Optional account ID to filter by
    ///
    /// # Returns
    /// Returns concentration data by asset category
    pub fn calculate_portfolio_concentration(
        &mut self,
        account_id: Option<Uuid>,
    ) -> Result<Vec<calculators_concentration::ConcentrationGroup>, Box<dyn std::error::Error>>
    {
        // Get all trades for the account
        let all_trades = if let Some(id) = account_id {
            // Get trades for specific account - need to get all statuses
            let mut trades = Vec::new();
            for status in model::Status::all() {
                if let Ok(mut status_trades) = self.search_trades(id, status) {
                    trades.append(&mut status_trades);
                }
            }
            trades
        } else {
            // Get trades for all accounts
            match self.search_all_accounts() {
                Ok(accounts) => {
                    let mut all_trades = Vec::new();
                    for account in accounts {
                        for status in model::Status::all() {
                            if let Ok(mut trades) = self.search_trades(account.id, status) {
                                all_trades.append(&mut trades);
                            }
                        }
                    }
                    all_trades
                }
                Err(e) => return Err(e),
            }
        };

        // Analyze concentration by asset class (primary analysis)
        let analysis = calculators_concentration::ConcentrationCalculator::analyze_by_metadata(
            &all_trades,
            calculators_concentration::MetadataField::AssetClass,
        );

        Ok(analysis.groups)
    }

    /// Get comprehensive trading summary combining all metrics
    ///
    /// # Arguments
    /// * `account_id` - Optional account ID to filter by (None for all accounts)
    ///
    /// # Returns
    /// Returns comprehensive trading summary data
    pub fn get_trading_summary(
        &mut self,
        account_id: Option<Uuid>,
    ) -> Result<TradingSummary, Box<dyn std::error::Error>> {
        if let Some(id) = account_id {
            let account_exists = self
                .factory
                .account_read()
                .all()?
                .iter()
                .any(|account| account.id == id);
            if !account_exists {
                return Err("Account not found".into());
            }
        }

        let equity = if let Some(id) = account_id {
            let balances = self.search_all_balances(id)?;
            balances
                .iter()
                .map(|balance| balance.total_balance)
                .fold(Decimal::ZERO, |acc, balance| {
                    acc.checked_add(balance).unwrap_or(acc)
                })
        } else {
            let accounts = self.search_all_accounts()?;
            accounts
                .iter()
                .map(|account| self.search_all_balances(account.id))
                .filter_map(Result::ok)
                .flat_map(|balances| balances.into_iter())
                .map(|balance| balance.total_balance)
                .fold(Decimal::ZERO, |acc, balance| {
                    acc.checked_add(balance).unwrap_or(acc)
                })
        };

        // Get performance stats from closed trades.
        let performance = match self.search_closed_trades(account_id) {
            Ok(closed_trades) => {
                if closed_trades.is_empty() {
                    None
                } else {
                    Some(
                        calculators_performance::PerformanceCalculator::calculate_performance_stats(
                            &closed_trades,
                        ),
                    )
                }
            }
            Err(_) => None,
        };

        let capital_at_risk = self
            .calculate_open_positions(account_id)
            .unwrap_or_else(|_| Vec::new());

        let concentration = self
            .calculate_portfolio_concentration(account_id)
            .unwrap_or_else(|_| Vec::new());

        Ok(TradingSummary {
            account_id,
            equity,
            performance,
            capital_at_risk,
            concentration,
        })
    }

    /// Creates a new account with hierarchy metadata.
    ///
    /// Only primary accounts receive a default level profile.
    #[allow(clippy::too_many_arguments)]
    pub fn create_account_with_hierarchy(
        &mut self,
        name: &str,
        description: &str,
        environment: Environment,
        taxes_percentage: Decimal,
        earnings_percentage: Decimal,
        account_type: AccountType,
        parent_account_id: Option<Uuid>,
    ) -> Result<Account, Box<dyn std::error::Error>> {
        self.consume_protected_authorization("create_account_with_hierarchy")?;
        let savepoint = "create_account_with_hierarchy";
        self.factory.begin_savepoint(savepoint)?;

        let account = match self.factory.account_write().create_with_hierarchy(
            name,
            description,
            environment,
            taxes_percentage,
            earnings_percentage,
            account_type,
            parent_account_id,
        ) {
            Ok(account) => account,
            Err(error) => {
                let _ = self.factory.rollback_to_savepoint(savepoint);
                return Err(error);
            }
        };

        if account.account_type == AccountType::Primary {
            if let Err(error) = self.factory.level_write().create_default_level(&account) {
                let _ = self.factory.rollback_to_savepoint(savepoint);
                return Err(error);
            }
        }

        self.factory.release_savepoint(savepoint)?;
        Ok(account)
    }

    /// Configure distribution rules for an account (DB-backed).
    pub fn configure_distribution(
        &mut self,
        account_id: Uuid,
        earnings_percent: Decimal,
        tax_percent: Decimal,
        reinvestment_percent: Decimal,
        minimum_threshold: Decimal,
        configuration_password: &str,
    ) -> Result<DistributionRules, Box<dyn std::error::Error>> {
        self.consume_protected_authorization("configure_distribution")?;
        // Validate percentages sum to 100%.
        let total = earnings_percent
            .checked_add(tax_percent)
            .and_then(|sum| sum.checked_add(reinvestment_percent))
            .ok_or("Arithmetic overflow in percentage calculation")?;
        if total != Decimal::ONE {
            return Err("Distribution percentages must sum to 100%".into());
        }

        let rules = DistributionRules::new(
            account_id,
            earnings_percent,
            tax_percent,
            reinvestment_percent,
            minimum_threshold,
        );
        rules.validate()?;

        match self.factory.distribution_read().for_account(account_id) {
            Ok(existing_rules) => {
                if !verify_distribution_password(
                    configuration_password,
                    &existing_rules.configuration_password_hash,
                )? {
                    return Err("Invalid distribution configuration password".into());
                }
            }
            Err(e) => {
                if !e.as_ref().is::<model::DistributionRulesNotFound>() {
                    return Err(e);
                }
            }
        }

        let password_hash = hash_distribution_password(configuration_password)?;
        self.factory.distribution_write().create_or_update(
            account_id,
            earnings_percent,
            tax_percent,
            reinvestment_percent,
            minimum_threshold,
            &password_hash,
        )
    }

    /// Execute profit distribution for an account using persisted rules.
    pub fn execute_distribution(
        &mut self,
        source_account_id: Uuid,
        profit_amount: Decimal,
        currency: Currency,
    ) -> Result<DistributionResult, Box<dyn std::error::Error>> {
        let source_account = self.factory.account_read().id(source_account_id)?;
        let rules = self
            .factory
            .distribution_read()
            .for_account(source_account_id)?;
        let (earnings_account, tax_account, reinvestment_account) =
            self.resolve_distribution_accounts(source_account_id)?;

        let mut distribution_service = ProfitDistributionService::new(&mut *self.factory);
        distribution_service.execute_distribution(
            &source_account,
            &earnings_account,
            &tax_account,
            &reinvestment_account,
            profit_amount,
            &rules,
            &currency,
            None,
        )
    }

    /// Returns persisted profit distribution execution history for an account.
    pub fn distribution_history(
        &mut self,
        source_account_id: Uuid,
    ) -> Result<Vec<DistributionHistory>, Box<dyn std::error::Error>> {
        self.factory
            .distribution_read()
            .history_for_account(source_account_id)
    }

    /// Transfer funds between accounts within the same hierarchy.
    pub fn transfer_between_accounts(
        &mut self,
        from_account_id: Uuid,
        to_account_id: Uuid,
        amount: Decimal,
        currency: Currency,
        reason: &str,
    ) -> Result<(Uuid, Uuid), Box<dyn std::error::Error>> {
        // Get accounts
        let from_account = self.factory.account_read().id(from_account_id)?;
        let to_account = self.factory.account_read().id(to_account_id)?;

        // Execute transfer
        let mut transfer_service = FundTransferService::new(&mut *self.factory);
        transfer_service.transfer_between_accounts(
            &from_account,
            &to_account,
            amount,
            &currency,
            reason,
        )
    }
}

impl TrustFacade {
    fn resolve_distribution_accounts(
        &mut self,
        source_account_id: Uuid,
    ) -> Result<(Account, Account, Account), Box<dyn std::error::Error>> {
        let child_accounts: Vec<Account> = self
            .factory
            .account_read()
            .all()?
            .into_iter()
            .filter(|account| account.parent_account_id == Some(source_account_id))
            .collect();

        let earnings_account = child_accounts
            .iter()
            .find(|account| account.account_type == AccountType::Earnings)
            .cloned()
            .ok_or("Missing earnings subaccount for distribution")?;
        let tax_account = child_accounts
            .iter()
            .find(|account| account.account_type == AccountType::TaxReserve)
            .cloned()
            .ok_or("Missing tax reserve subaccount for distribution")?;
        let reinvestment_account = child_accounts
            .iter()
            .find(|account| account.account_type == AccountType::Reinvestment)
            .cloned()
            .ok_or("Missing reinvestment subaccount for distribution")?;

        Ok((earnings_account, tax_account, reinvestment_account))
    }
}

fn hash_distribution_password(password: &str) -> Result<String, Box<dyn std::error::Error>> {
    if password.trim().len() < 8 {
        return Err("Distribution password must be at least 8 characters".into());
    }

    let salt = SaltString::generate(&mut OsRng);
    let argon2 = Argon2::default();
    let hash = argon2
        .hash_password(password.as_bytes(), &salt)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e.to_string()))?;
    Ok(hash.to_string())
}

fn verify_distribution_password(
    password: &str,
    stored_hash: &str,
) -> Result<bool, Box<dyn std::error::Error>> {
    if stored_hash.starts_with("$argon2") {
        let parsed = PasswordHash::new(stored_hash).map_err(|e| {
            std::io::Error::new(
                std::io::ErrorKind::Other,
                format!("Invalid password hash: {e}"),
            )
        })?;
        Ok(Argon2::default()
            .verify_password(password.as_bytes(), &parsed)
            .is_ok())
    } else {
        Ok(hash_distribution_password_legacy_sha256(password)? == stored_hash)
    }
}

fn hash_distribution_password_legacy_sha256(
    password: &str,
) -> Result<String, Box<dyn std::error::Error>> {
    if password.trim().len() < 8 {
        return Err("Distribution password must be at least 8 characters".into());
    }
    let mut hasher = Sha256::new();
    hasher.update(password.as_bytes());
    let digest = hasher.finalize();
    Ok(format!("{digest:x}"))
}

mod calculators_account;
pub mod calculators_advanced_metrics;
pub mod calculators_concentration;
pub mod calculators_drawdown;
pub mod calculators_performance;
pub mod calculators_risk;
mod calculators_trade;
mod commands;
/// Domain events used by core workflows.
pub mod events;
mod mocks;
/// Core service layer modules.
pub mod services;
mod validators;
