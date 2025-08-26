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
use calculators_trade::QuantityCalculator;
use model::{
    Account, AccountBalance, AccountType, Broker, BrokerLog, Currency, DatabaseFactory,
    DistributionResult, DistributionRules, DraftTrade, Environment, Order, Rule, RuleLevel,
    RuleName, Status, Trade, TradeBalance, TradingVehicle, TradingVehicleCategory, Transaction,
    TransactionCategory,
};
use rust_decimal::Decimal;
use uuid::Uuid;

/// The main facade for interacting with the Trust financial trading system.
///
/// This struct provides a unified interface for all core operations including
/// account management, trade execution, risk management, and transaction handling.
/// It encapsulates the database factory and broker implementations.
pub struct TrustFacade {
    factory: Box<dyn DatabaseFactory>,
    broker: Box<dyn Broker>,
}

impl std::fmt::Debug for TrustFacade {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TrustFacade")
            .field("factory", &"Box<dyn DatabaseFactory>")
            .field("broker", &"Box<dyn Broker>")
            .finish()
    }
}

/// Trust is the main entry point for interacting with the core library.
/// It is a facade that provides a simple interface for interacting with the
/// core library.
impl TrustFacade {
    /// Creates a new instance of Trust.
    pub fn new(factory: Box<dyn DatabaseFactory>, broker: Box<dyn Broker>) -> Self {
        TrustFacade { factory, broker }
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
        self.factory.account_write().create(
            name,
            description,
            environment,
            taxes_percentage,
            earnings_percentage,
        )
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
        isin: &str,
        category: &TradingVehicleCategory,
        broker: &str,
    ) -> Result<TradingVehicle, Box<dyn std::error::Error>> {
        self.factory
            .trading_vehicle_write()
            .create_trading_vehicle(symbol, isin, category, broker)
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
        QuantityCalculator::maximum_quantity(
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
        commands::trade::sync_with_broker(trade, account, &mut *self.factory, &mut *self.broker)
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
        commands::trade::stop_acquired(trade, fee, &mut *self.factory)
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
        commands::trade::close(trade, &mut *self.factory, &mut *self.broker)
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
        commands::trade::target_acquired(trade, fee, &mut *self.factory)
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

    /// Creates a new account with hierarchy support
    /// Note: For now, uses existing create method - hierarchy will be added when database layer is updated
    pub fn create_account_with_hierarchy(
        &mut self,
        name: &str,
        description: &str,
        environment: Environment,
        taxes_percentage: Decimal,
        earnings_percentage: Decimal,
        _account_type: AccountType,
        _parent_account_id: Option<Uuid>,
    ) -> Result<Account, Box<dyn std::error::Error>> {
        // TODO: Update when database layer supports hierarchy
        self.factory.account_write().create(
            name,
            description,
            environment,
            taxes_percentage,
            earnings_percentage,
        )
    }

    /// Configure distribution rules for an account
    /// Note: For now, only validates rules - database storage will be added when database layer is updated
    pub fn configure_distribution(
        &mut self,
        account_id: Uuid,
        earnings_percent: Decimal,
        tax_percent: Decimal,
        reinvestment_percent: Decimal,
        minimum_threshold: Decimal,
    ) -> Result<DistributionRules, Box<dyn std::error::Error>> {
        // Validate percentages sum to 100%
        let total = earnings_percent + tax_percent + reinvestment_percent;
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

        // Validate the rules
        rules.validate()?;

        // TODO: Save to database when distribution_write trait is updated
        // self.factory.distribution_write().create_rules(&rules)?;
        Ok(rules)
    }

    /// Execute profit distribution for an account
    /// Note: For now, uses default rules - database integration will be added when database layer is updated
    pub fn execute_distribution(
        &mut self,
        source_account_id: Uuid,
        earnings_account_id: Uuid,
        tax_account_id: Uuid,
        reinvestment_account_id: Uuid,
        profit_amount: Decimal,
        currency: Currency,
    ) -> Result<DistributionResult, Box<dyn std::error::Error>> {
        // Get accounts
        let source_account = self.factory.account_read().id(source_account_id)?;
        let earnings_account = self.factory.account_read().id(earnings_account_id)?;
        let tax_account = self.factory.account_read().id(tax_account_id)?;
        let reinvestment_account = self.factory.account_read().id(reinvestment_account_id)?;

        // TODO: Get distribution rules from database when implemented
        // let rules = self.factory.distribution_read().for_account(source_account_id)?;

        // Use default rules for now (40% earnings, 30% tax, 30% reinvestment)
        let rules = DistributionRules::new(
            source_account_id,
            Decimal::new(40, 2),  // 40%
            Decimal::new(30, 2),  // 30%
            Decimal::new(30, 2),  // 30%
            Decimal::new(100, 0), // $100 minimum
        );

        // Execute distribution
        let mut distribution_service = ProfitDistributionService::new(&mut *self.factory);
        distribution_service.execute_distribution(
            &source_account,
            &earnings_account,
            &tax_account,
            &reinvestment_account,
            profit_amount,
            &rules,
            &currency,
        )
    }

    /// Transfer funds between accounts in hierarchy
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

mod calculators_account;
pub mod calculators_drawdown;
pub mod calculators_performance;
pub mod calculators_risk;
mod calculators_trade;
mod commands;
/// Integration tests module for validating complete system workflows
pub mod integration_tests;
mod mocks;
pub mod services;
mod validators;
