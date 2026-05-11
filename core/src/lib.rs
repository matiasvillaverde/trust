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

use crate::services::{
    AdvisoryHistoryEntry, AdvisoryResult, AdvisoryThresholds, FundTransferService,
    PortfolioAdvisoryStatus, ProfitDistributionService, TradeProposal,
};
use advisor::{
    CalendarCredentials, CatalystScanRequest, CatalystScanResult, CatalystScanner,
    CorrelationAdvisory, CorrelationCalculator, CorrelationConfig, CorrelationRequest,
    RegimeConfig, RegimeFilter, RegimeRequest, RegimeSnapshot,
};
use argon2::{
    password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString},
    Argon2,
};
use broker_registry::BrokerRegistry;
use calculators_fixed_income::{BondAnalytics, BondAnalyticsInput, FixedIncomeCalculator};
use calculators_performance::{BiasAggregation, BiasAggregationCalculator};
use calculators_trade::{
    LevelAdjustedQuantity, QuantityCalculator, TradeHypothesis, TradeHypothesisCalculator,
};
use events::trade::{CloseReason, TradeClosed};
use model::database::TradingVehicleUpsert;
use model::{
    Account, AccountBalance, AccountType, BarTimeframe, Broker, BrokerKind, BrokerLog, Currency,
    DatabaseFactory, DistributionHistory, DistributionResult, DistributionRules, DraftTrade,
    Environment, Execution, Level, LevelAdjustmentRules, LevelChange, LevelTrigger, MarketBar,
    MarketDataChannel, MarketDataStreamEvent, MarketSnapshot, MarketSnapshotSource,
    MarketSnapshotV2, Mistake, Order, Rule, RuleLevel, RuleName, SessionPlan, SessionPlanClose,
    Status, Trade, TradeBalance, TradeEvent, TradeEventSeverity, TradeEventSource, TradeEventType,
    TradingVehicle, TradingVehicleCategory, Transaction, TransactionCategory,
};
use rand_core::OsRng;
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use uuid::Uuid;
use {
    services::leveling::{
        DefaultLevelTransitionPolicy, LevelEvaluationOutcome, LevelPerformanceSnapshot,
        LevelingService,
    },
    std::collections::HashMap,
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
    distribution_rules_cache: HashMap<Uuid, Option<DistributionRules>>,
    level_snapshot_cache: HashMap<(Uuid, Currency), LevelSnapshotCache>,
    advisory_history: Vec<AdvisoryHistoryEntry>,
}

#[derive(Debug, Clone)]
struct LevelSnapshotCache {
    points: std::collections::VecDeque<(chrono::NaiveDateTime, Decimal)>,
    sum_performance: Decimal,
    min_performance: Option<Decimal>,
    profitable_trades: u32,
    consecutive_wins: u32,
    last_closed_at: Option<chrono::NaiveDateTime>,
}

impl LevelSnapshotCache {
    fn new() -> Self {
        Self {
            points: std::collections::VecDeque::new(),
            sum_performance: Decimal::ZERO,
            min_performance: None,
            profitable_trades: 0,
            consecutive_wins: 0,
            last_closed_at: None,
        }
    }

    fn seed_from_points(&mut self, points: Vec<(chrono::NaiveDateTime, Decimal)>) {
        self.points = points.into_iter().collect();
        self.last_closed_at = self.points.back().map(|(ts, _)| *ts);
        self.recompute_aggregates();
    }

    fn push_and_prune(&mut self, closed_at: chrono::NaiveDateTime, performance: Decimal) {
        let cutoff = closed_at
            .checked_sub_signed(chrono::Duration::days(
                LevelingService::<DefaultLevelTransitionPolicy>::EVALUATION_WINDOW_DAYS,
            ))
            .unwrap_or(closed_at);

        // Prune oldest.
        while let Some((ts, value)) = self.points.front().cloned() {
            if ts >= cutoff {
                break;
            }
            self.points.pop_front();
            self.sum_performance = self
                .sum_performance
                .checked_sub(value)
                .unwrap_or(self.sum_performance);
            if value > Decimal::ZERO {
                self.profitable_trades = self.profitable_trades.saturating_sub(1);
            }
            if self.min_performance == Some(value) {
                // Recompute min lazily when the current min falls out of window.
                self.min_performance = None;
                for (_, v) in &self.points {
                    self.min_performance = Some(match self.min_performance {
                        None => *v,
                        Some(min) => {
                            if *v < min {
                                *v
                            } else {
                                min
                            }
                        }
                    });
                }
            }
        }

        // Append newest.
        self.points.push_back((closed_at, performance));
        self.sum_performance = self
            .sum_performance
            .checked_add(performance)
            .unwrap_or(self.sum_performance);
        if performance > Decimal::ZERO {
            self.profitable_trades = self.profitable_trades.saturating_add(1);
        }
        self.min_performance = Some(match self.min_performance {
            None => performance,
            Some(min) => {
                if performance < min {
                    performance
                } else {
                    min
                }
            }
        });

        // Update consecutive wins. If timestamps are out of order, fall back to recompute.
        let in_order = self
            .last_closed_at
            .map(|prev| closed_at >= prev)
            .unwrap_or(true);
        self.last_closed_at = Some(closed_at);
        if in_order {
            if performance > Decimal::ZERO {
                self.consecutive_wins = self.consecutive_wins.saturating_add(1);
            } else {
                self.consecutive_wins = 0;
            }
        } else {
            self.recompute_consecutive_wins();
        }
    }

    fn recompute_aggregates(&mut self) {
        self.sum_performance = Decimal::ZERO;
        self.min_performance = None;
        self.profitable_trades = 0;
        for (_, v) in &self.points {
            self.sum_performance = self
                .sum_performance
                .checked_add(*v)
                .unwrap_or(self.sum_performance);
            if *v > Decimal::ZERO {
                self.profitable_trades = self.profitable_trades.saturating_add(1);
            }
            self.min_performance = Some(match self.min_performance {
                None => *v,
                Some(min) => {
                    if *v < min {
                        *v
                    } else {
                        min
                    }
                }
            });
        }
        self.recompute_consecutive_wins();
    }

    fn recompute_consecutive_wins(&mut self) {
        let mut wins = 0u32;
        for (_, v) in self.points.iter().rev() {
            if *v > Decimal::ZERO {
                wins = wins.saturating_add(1);
            } else {
                break;
            }
        }
        self.consecutive_wins = wins;
    }

    fn snapshot(&self, baseline: Decimal) -> LevelPerformanceSnapshot {
        let total_trades = u32::try_from(self.points.len()).unwrap_or(u32::MAX);
        let total_trades_dec = Decimal::from(total_trades);
        let win_rate_percentage = if total_trades_dec > Decimal::ZERO {
            Decimal::from(self.profitable_trades)
                .checked_div(total_trades_dec)
                .and_then(|ratio| ratio.checked_mul(dec!(100)))
                .unwrap_or(Decimal::ZERO)
        } else {
            Decimal::ZERO
        };

        let monthly_loss_percentage =
            if self.sum_performance < Decimal::ZERO && baseline > Decimal::ZERO {
                self.sum_performance
                    .checked_div(baseline)
                    .and_then(|ratio| ratio.checked_mul(dec!(100)))
                    .unwrap_or(Decimal::ZERO)
            } else {
                Decimal::ZERO
            };

        let largest_loss = self.min_performance.unwrap_or(Decimal::ZERO);
        let largest_loss_percentage = if largest_loss < Decimal::ZERO && baseline > Decimal::ZERO {
            largest_loss
                .checked_div(baseline)
                .and_then(|ratio| ratio.checked_mul(dec!(100)))
                .unwrap_or(Decimal::ZERO)
        } else {
            Decimal::ZERO
        };

        LevelPerformanceSnapshot {
            profitable_trades: self.profitable_trades,
            win_rate_percentage,
            monthly_loss_percentage,
            largest_loss_percentage,
            consecutive_wins: self.consecutive_wins,
        }
    }
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

fn protected_keyword_matches(expected: &str, provided: &str) -> bool {
    if expected.is_empty() || provided.is_empty() {
        return false;
    }

    let expected_bytes = expected.as_bytes();
    let provided_bytes = provided.as_bytes();
    let max_len = expected_bytes.len().max(provided_bytes.len());
    let mut diff = expected_bytes.len() ^ provided_bytes.len();

    for index in 0..max_len {
        let expected_byte = expected_bytes.get(index).copied().unwrap_or_default();
        let provided_byte = provided_bytes.get(index).copied().unwrap_or_default();
        diff |= usize::from(expected_byte ^ provided_byte);
    }

    diff == 0
}

/// Trust is the main entry point for interacting with the core library.
/// It is a facade that provides a simple interface for interacting with the
/// core library.
impl TrustFacade {
    /// Creates a new instance of Trust.
    pub fn new(factory: Box<dyn DatabaseFactory>, broker: Box<dyn Broker>) -> Self {
        TrustFacade {
            factory,
            broker: Box::new(BrokerRegistry::from_single(broker)),
            protected_mode: false,
            protected_authorized: false,
            distribution_rules_cache: HashMap::new(),
            level_snapshot_cache: HashMap::new(),
            advisory_history: Vec::new(),
        }
    }

    /// Creates a new instance of Trust with broker routing per account.
    pub fn new_with_brokers(
        factory: Box<dyn DatabaseFactory>,
        brokers: Vec<Box<dyn Broker>>,
    ) -> Self {
        Self::new(factory, Box::new(BrokerRegistry::from_many(brokers)))
    }

    /// Enables protected-mutation enforcement for this facade instance.
    pub fn enable_protected_mode(&mut self) {
        self.protected_mode = true;
    }

    /// Authorizes exactly one protected mutation operation with the protected keyword.
    pub fn authorize_protected_mutation(
        &mut self,
        provided_keyword: &str,
        expected_keyword: &str,
    ) -> Result<(), Box<dyn std::error::Error>> {
        if !self.protected_mode {
            return Ok(());
        }
        if !protected_keyword_matches(expected_keyword, provided_keyword) {
            return Err("Protected mutation authorization failed".into());
        }
        self.protected_authorized = true;
        Ok(())
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

    fn ensure_account_exists(
        &mut self,
        account_id: Uuid,
    ) -> Result<(), Box<dyn std::error::Error>> {
        if self
            .search_all_accounts()?
            .into_iter()
            .any(|account| account.id == account_id)
        {
            return Ok(());
        }
        Err(format!("account not found: {account_id}").into())
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
        self.create_account_with_profile(
            name,
            description,
            environment,
            taxes_percentage,
            earnings_percentage,
            AccountType::Primary,
            None,
            BrokerKind::Alpaca,
            None,
        )
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

    /// Soft-delete an account after protected-mode and database safety checks.
    ///
    /// `force` bypasses only the zero-balance check; open trades and active child
    /// accounts remain protected by the database implementation.
    pub fn delete_account(
        &mut self,
        account_id: Uuid,
        force: bool,
    ) -> Result<Account, Box<dyn std::error::Error>> {
        self.consume_protected_authorization("delete_account")?;
        let savepoint = "delete_account";
        self.factory.begin_savepoint(savepoint)?;

        let deleted = match self.factory.account_write().delete(account_id, force) {
            Ok(account) => account,
            Err(error) => {
                let _ = self.factory.rollback_to_savepoint(savepoint);
                return Err(error);
            }
        };

        self.factory.release_savepoint(savepoint)?;
        Ok(deleted)
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
    ) -> Result<Decimal, Box<dyn std::error::Error>> {
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

    /// Calculate trade hypothesis metrics for a proposed position size.
    pub fn calculate_trade_hypothesis(
        &mut self,
        account_id: Uuid,
        entry_price: Decimal,
        stop_price: Decimal,
        target_price: Decimal,
        quantity: Decimal,
        currency: &Currency,
    ) -> Result<TradeHypothesis, Box<dyn std::error::Error>> {
        TradeHypothesisCalculator::calculate(
            account_id,
            entry_price,
            stop_price,
            target_price,
            quantity,
            currency,
            &mut *self.factory,
        )
    }

    /// Calculate fixed-income analytics for a plain-vanilla bond position.
    pub fn calculate_bond_analytics(
        &self,
        input: BondAnalyticsInput,
    ) -> Result<BondAnalytics, calculators_fixed_income::FixedIncomeError> {
        FixedIncomeCalculator::analyze_bond(input)
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

    /// Create a new trade and choose the safety order type.
    pub fn create_trade_with_safety_order_category(
        &mut self,
        trade: DraftTrade,
        stop_price: Decimal,
        entry_price: Decimal,
        target_price: Decimal,
        safety_order_category: model::OrderCategory,
    ) -> Result<Trade, Box<dyn std::error::Error>> {
        commands::trade::create_trade_with_safety_order_category(
            trade,
            stop_price,
            entry_price,
            target_price,
            safety_order_category,
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

    /// Read a trade by its identifier.
    pub fn read_trade(&mut self, trade_id: Uuid) -> Result<Trade, Box<dyn std::error::Error>> {
        self.factory.trade_read().read_trade(trade_id)
    }

    /// Persist a manually entered catalyst event for a trade.
    pub fn create_trade_event(
        &mut self,
        trade_id: Uuid,
        event_type: TradeEventType,
        event_date: chrono::NaiveDate,
        severity: TradeEventSeverity,
        notes: Option<String>,
    ) -> Result<TradeEvent, Box<dyn std::error::Error>> {
        let trade = self.read_trade(trade_id)?;
        let now = chrono::Utc::now().naive_utc();
        let event = TradeEvent {
            id: Uuid::new_v4(),
            created_at: now,
            updated_at: now,
            deleted_at: None,
            trade_id,
            symbol: trade.trading_vehicle.symbol,
            event_type,
            event_date,
            severity,
            notes,
            source: TradeEventSource::Manual,
        };

        self.factory.trade_event_write().create_trade_event(&event)
    }

    /// Read active catalyst events for a trade.
    pub fn trade_events_for_trade(
        &mut self,
        trade_id: Uuid,
    ) -> Result<Vec<TradeEvent>, Box<dyn std::error::Error>> {
        self.read_trade(trade_id)?;
        self.factory
            .trade_event_read()
            .read_trade_events_for_trade(trade_id)
    }

    /// Persist a post-trade mistake review for a closed, graded trade.
    pub fn create_mistake(
        &mut self,
        mistake: Mistake,
    ) -> Result<Mistake, Box<dyn std::error::Error>> {
        self.read_trade(mistake.trade_id)?;
        self.factory.mistake_write().create_mistake(&mistake)
    }

    /// Read active post-trade mistakes for a trade.
    pub fn mistakes_for_trade(
        &mut self,
        trade_id: Uuid,
    ) -> Result<Vec<Mistake>, Box<dyn std::error::Error>> {
        self.read_trade(trade_id)?;
        self.factory
            .mistake_read()
            .read_mistakes_for_trade(trade_id)
    }

    /// Aggregate post-trade mistake bias patterns for an account over a lookback window.
    pub fn bias_aggregation_for_account(
        &mut self,
        account_id: Uuid,
        window_days: i32,
    ) -> Result<BiasAggregation, Box<dyn std::error::Error>> {
        if window_days <= 0 {
            return Err("bias aggregation window_days must be positive".into());
        }

        self.ensure_account_exists(account_id)?;
        let end_at = chrono::Utc::now().naive_utc();
        let start_at = end_at
            .checked_sub_signed(chrono::Duration::days(i64::from(window_days)))
            .ok_or("bias aggregation window is out of range")?;
        let mistakes = self
            .factory
            .mistake_read()
            .read_mistakes_for_account_in_period(account_id, start_at, end_at)?;

        Ok(BiasAggregationCalculator::calculate(&mistakes, window_days))
    }

    /// Persist a new open plan-act-review session plan.
    pub fn create_session_plan(
        &mut self,
        session_plan: SessionPlan,
    ) -> Result<SessionPlan, Box<dyn std::error::Error>> {
        self.ensure_account_exists(session_plan.account_id)?;
        if self
            .factory
            .session_plan_read()
            .read_open_session(session_plan.account_id)?
            .is_some()
        {
            return Err("account already has an open session".into());
        }
        self.factory
            .session_plan_write()
            .create_session_plan(&session_plan)
    }

    /// Read the active open session plan for an account, if present.
    pub fn open_session_for_account(
        &mut self,
        account_id: Uuid,
    ) -> Result<Option<SessionPlan>, Box<dyn std::error::Error>> {
        self.ensure_account_exists(account_id)?;
        self.factory
            .session_plan_read()
            .read_open_session(account_id)
    }

    /// Read active session plans for an account in an inclusive opened-at period.
    pub fn session_plans_for_account(
        &mut self,
        account_id: Uuid,
        start_at: chrono::NaiveDateTime,
        end_at: chrono::NaiveDateTime,
    ) -> Result<Vec<SessionPlan>, Box<dyn std::error::Error>> {
        self.ensure_account_exists(account_id)?;
        self.factory
            .session_plan_read()
            .read_session_plans_for_account(account_id, start_at, end_at)
    }

    /// Close an open session plan by applying post-session review fields.
    pub fn close_session_plan(
        &mut self,
        close: SessionPlanClose,
    ) -> Result<SessionPlan, Box<dyn std::error::Error>> {
        self.factory.session_plan_write().close_session_plan(&close)
    }

    /// Read trades created for an account during an inclusive session window.
    pub fn trades_for_account_in_period(
        &mut self,
        account_id: Uuid,
        start_at: chrono::NaiveDateTime,
        end_at: chrono::NaiveDateTime,
    ) -> Result<Vec<Trade>, Box<dyn std::error::Error>> {
        if end_at < start_at {
            return Err("trade period end_at cannot be before start_at".into());
        }
        self.ensure_account_exists(account_id)?;

        let mut trades = Vec::new();
        for status in Status::all() {
            let mut status_trades = self.search_trades(account_id, status)?;
            trades.append(&mut status_trades);
        }
        trades.retain(|trade| trade.created_at >= start_at && trade.created_at <= end_at);
        trades.sort_by_key(|trade| (trade.created_at, trade.id));
        Ok(trades)
    }

    /// Scan external calendar catalysts for a trade and persist returned events.
    pub fn scan_trade_catalysts(
        &mut self,
        request: &CatalystScanRequest,
        credentials: CalendarCredentials,
    ) -> Result<CatalystScanResult, Box<dyn std::error::Error>> {
        let scanner = CatalystScanner::new(credentials);
        let mut writer = self.factory.trade_event_write();
        Ok(scanner.scan(request, &mut *writer)?)
    }

    /// Compute broker-bar correlation advisory data for a candidate trade.
    pub fn correlation_advisory(
        &mut self,
        request: &CorrelationRequest,
        account: &Account,
        config: CorrelationConfig,
    ) -> Result<CorrelationAdvisory, Box<dyn std::error::Error>> {
        let calculator = CorrelationCalculator::new(config)?;
        Ok(calculator.analyze(request, &*self.broker, account)?)
    }

    /// Compute broker-bar market-regime advisory data.
    pub fn regime_advisory(
        &mut self,
        request: &RegimeRequest,
        account: &Account,
        config: RegimeConfig,
    ) -> Result<RegimeSnapshot, Box<dyn std::error::Error>> {
        let filter = RegimeFilter::new(config)?;
        Ok(filter.evaluate(request, &*self.broker, account)?)
    }

    /// Fetch historical market bars from the configured broker market-data API.
    pub fn market_bars(
        &mut self,
        account: &Account,
        symbol: &str,
        start: chrono::DateTime<chrono::Utc>,
        end: chrono::DateTime<chrono::Utc>,
        timeframe: BarTimeframe,
    ) -> Result<Vec<MarketBar>, Box<dyn std::error::Error>> {
        self.broker.get_bars(symbol, start, end, timeframe, account)
    }

    /// Fetch a best-effort market snapshot derived from the latest minute bar.
    ///
    /// This method uses the latest available one-minute bar as a compact,
    /// broker-agnostic snapshot representation.
    pub fn market_snapshot(
        &mut self,
        account: &Account,
        symbol: &str,
        now: chrono::DateTime<chrono::Utc>,
    ) -> Result<MarketSnapshot, Box<dyn std::error::Error>> {
        let start = now
            .checked_sub_signed(chrono::Duration::minutes(30))
            .unwrap_or(now);
        let bars = self
            .broker
            .get_bars(symbol, start, now, BarTimeframe::OneMinute, account)?;

        let latest = bars
            .into_iter()
            .max_by_key(|bar| bar.time)
            .ok_or_else(|| "No market bars returned for snapshot request".to_string())?;

        Ok(MarketSnapshot {
            symbol: symbol.to_string(),
            as_of: latest.time,
            last_price: latest.close,
            volume: latest.volume,
            open: latest.open,
            high: latest.high,
            low: latest.low,
        })
    }

    /// Fetch a richer market snapshot with quote/trade enrichment when supported.
    ///
    /// If quote/trade retrieval is unavailable, this gracefully falls back to
    /// bar-derived snapshot semantics.
    pub fn market_snapshot_v2(
        &mut self,
        account: &Account,
        symbol: &str,
        now: chrono::DateTime<chrono::Utc>,
    ) -> Result<MarketSnapshotV2, Box<dyn std::error::Error>> {
        let quote = self.broker.get_latest_quote(symbol, account).ok();
        let trade = self.broker.get_latest_trade(symbol, account).ok();

        if let (Some(quote), Some(trade)) = (quote, trade) {
            let as_of = if quote.as_of >= trade.as_of {
                quote.as_of
            } else {
                trade.as_of
            };
            let last_price = trade.price;
            let volume = trade.size;
            let open = last_price;
            let high = if quote.ask_price >= last_price {
                quote.ask_price
            } else {
                last_price
            };
            let low = if quote.bid_price <= last_price {
                quote.bid_price
            } else {
                last_price
            };
            return Ok(MarketSnapshotV2 {
                symbol: symbol.to_string(),
                as_of,
                last_price,
                volume,
                open,
                high,
                low,
                quote: Some(quote),
                trade: Some(trade),
                source: MarketSnapshotSource::QuoteTrade,
            });
        }

        let fallback = self.market_snapshot(account, symbol, now)?;
        Ok(MarketSnapshotV2 {
            symbol: fallback.symbol,
            as_of: fallback.as_of,
            last_price: fallback.last_price,
            volume: fallback.volume,
            open: fallback.open,
            high: fallback.high,
            low: fallback.low,
            quote: None,
            trade: None,
            source: MarketSnapshotSource::BarsFallback,
        })
    }

    /// Retrieve a finite batch of realtime market-data events.
    pub fn stream_market_data(
        &mut self,
        account: &Account,
        symbols: &[String],
        channels: &[MarketDataChannel],
        max_events: usize,
        timeout_seconds: u64,
    ) -> Result<Vec<MarketDataStreamEvent>, Box<dyn std::error::Error>> {
        self.broker
            .stream_market_data(symbols, channels, max_events, timeout_seconds, account)
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
        let (status, orders, log, transitioned_to_closed, persisted_trade) =
            commands::trade::sync_with_broker(
                trade,
                account,
                &mut *self.factory,
                &mut *self.broker,
            )?;

        if transitioned_to_closed {
            // Close-event handler (leveling, distribution, etc). We keep this best-effort so the
            // sync path remains reliable, but individual components can still surface errors in
            // direct/manual flows.
            let close_reason = if status == Status::ClosedTarget {
                CloseReason::Target
            } else {
                CloseReason::StopLoss
            };
            let _ = self.handle_trade_closed_event_from_trade(&persisted_trade, close_reason);

            // Auto-grading on close is enabled by default; we keep it best-effort for sync reliability.
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
        self.handle_trade_closed_event(trade.id, CloseReason::StopLoss)?;
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

        // 2. Read persisted post-close state and trigger distribution from fresh data.
        let closed_trade = self.factory.trade_read().read_trade(trade.id)?;
        let distribution_result = self.try_auto_distribute_profit_for_trade(&closed_trade)?;

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
        self.handle_trade_closed_event(trade.id, CloseReason::Target)?;
        Ok(result)
    }

    fn handle_trade_closed_event(
        &mut self,
        trade_id: Uuid,
        close_reason: CloseReason,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let trade = self.factory.trade_read().read_trade(trade_id)?;
        self.handle_trade_closed_event_from_trade(&trade, close_reason)
    }

    fn handle_trade_closed_event_from_trade(
        &mut self,
        trade: &Trade,
        close_reason: CloseReason,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let risk_per_share = match trade
            .entry
            .unit_price
            .checked_sub(trade.safety_stop.unit_price)
        {
            Some(value) if value > Decimal::ZERO => value,
            _ => Decimal::ZERO,
        };
        let qty = trade.entry.quantity;
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

        let snapshot = self.cached_level_snapshot_for_trade_close(trade)?;
        let service = self.leveling_service_for_account(trade.account_id)?;
        let _ = service.handle_trade_closed_with_snapshot(&mut *self.factory, &event, &snapshot)?;

        // Trigger profit distribution (if configured) based on persisted post-close state.
        // We cache rule lookups to keep the sync/close hot path performant.
        let _ = self.try_auto_distribute_profit_for_trade(trade)?;
        Ok(())
    }

    fn cached_level_snapshot_for_trade_close(
        &mut self,
        trade: &Trade,
    ) -> Result<LevelPerformanceSnapshot, Box<dyn std::error::Error>> {
        let key = (trade.account_id, trade.currency);
        let closed_at = trade.updated_at;

        let mut seeded = false;
        if !self.level_snapshot_cache.contains_key(&key) {
            let cutoff = closed_at
                .checked_sub_signed(chrono::Duration::days(
                    LevelingService::<DefaultLevelTransitionPolicy>::EVALUATION_WINDOW_DAYS,
                ))
                .unwrap_or(closed_at);
            let points = self
                .factory
                .trade_read()
                .read_recent_closed_trade_performance_points(
                    trade.account_id,
                    &trade.currency,
                    cutoff,
                )?;

            let mut cache = LevelSnapshotCache::new();
            cache.seed_from_points(points);
            self.level_snapshot_cache.insert(key, cache);
            seeded = true;
        }

        let cache = self
            .level_snapshot_cache
            .get_mut(&key)
            .ok_or_else(|| "level snapshot cache missing entry after insert".to_string())?;

        if seeded {
            // When seeding from DB, closed stop/target trades are already included in the points query.
            // Manual closes (e.g. canceled) are not; we need to push them explicitly.
            let is_db_closed =
                trade.status == Status::ClosedTarget || trade.status == Status::ClosedStopLoss;
            let db_includes_trigger =
                is_db_closed && cache.points.back().is_some_and(|(ts, _)| *ts == closed_at);
            if !db_includes_trigger {
                cache.push_and_prune(closed_at, trade.balance.total_performance);
            }
        } else {
            // Incremental update for subsequent closes.
            cache.push_and_prune(closed_at, trade.balance.total_performance);
        }

        let baseline = self
            .factory
            .account_balance_read()
            .for_currency(trade.account_id, &trade.currency)
            .map(|balance| balance.total_balance)
            .unwrap_or(dec!(1));
        let baseline = if baseline > Decimal::ZERO {
            baseline
        } else {
            dec!(1)
        };

        Ok(cache.snapshot(baseline))
    }

    fn cached_distribution_rules(
        &mut self,
        account_id: Uuid,
    ) -> Result<Option<DistributionRules>, Box<dyn std::error::Error>> {
        if let Some(cached) = self.distribution_rules_cache.get(&account_id) {
            return Ok(cached.clone());
        }

        let rules = match self.factory.distribution_read().for_account(account_id) {
            Ok(rules) => Some(rules),
            Err(error) => {
                if error.as_ref().is::<model::DistributionRulesNotFound>() {
                    None
                } else {
                    return Err(error);
                }
            }
        };

        self.distribution_rules_cache
            .insert(account_id, rules.clone());
        Ok(rules)
    }

    fn try_auto_distribute_profit_for_trade(
        &mut self,
        trade: &Trade,
    ) -> Result<Option<DistributionResult>, Box<dyn std::error::Error>> {
        let profit = trade.balance.total_performance;
        if profit <= Decimal::ZERO {
            return Ok(None);
        }

        let Some(rules) = self.cached_distribution_rules(trade.account_id)? else {
            return Ok(None);
        };

        if profit < rules.minimum_threshold {
            return Ok(None);
        }

        let source_account = self.factory.account_read().id(trade.account_id)?;
        let (earnings_account, tax_account, reinvestment_account) =
            self.resolve_distribution_accounts(source_account.id)?;
        let mut distribution_service = ProfitDistributionService::new(&mut *self.factory);

        let result = distribution_service.execute_distribution(
            &source_account,
            &earnings_account,
            &tax_account,
            &reinvestment_account,
            profit,
            &rules,
            &trade.currency,
            Some(trade.id),
        )?;

        Ok(Some(result))
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
        self.create_account_with_profile(
            name,
            description,
            environment,
            taxes_percentage,
            earnings_percentage,
            account_type,
            parent_account_id,
            BrokerKind::Alpaca,
            None,
        )
    }

    /// Creates a new account with hierarchy and broker-profile metadata.
    #[allow(clippy::too_many_arguments)]
    pub fn create_account_with_profile(
        &mut self,
        name: &str,
        description: &str,
        environment: Environment,
        taxes_percentage: Decimal,
        earnings_percentage: Decimal,
        account_type: AccountType,
        parent_account_id: Option<Uuid>,
        broker_kind: BrokerKind,
        broker_account_id: Option<&str>,
    ) -> Result<Account, Box<dyn std::error::Error>> {
        self.consume_protected_authorization("create_account_with_profile")?;
        let savepoint = "create_account_with_profile";
        self.factory.begin_savepoint(savepoint)?;

        let account = match self.factory.account_write().create_with_profile(
            name,
            description,
            environment,
            taxes_percentage,
            earnings_percentage,
            account_type,
            parent_account_id,
            broker_kind,
            broker_account_id,
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

        // Existing rules can only be updated with the existing configuration password.
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
                // Treat only explicit not-found as "no rules configured"; propagate all other errors.
                if !e.as_ref().is::<model::DistributionRulesNotFound>() {
                    return Err(e);
                }
            }
        }

        let password_hash = hash_distribution_password(configuration_password)?;
        let rules = self.factory.distribution_write().create_or_update(
            account_id,
            earnings_percent,
            tax_percent,
            reinvestment_percent,
            minimum_threshold,
            &password_hash,
        )?;

        // Cache for this facade instance to avoid repeated DB reads on high-frequency closes.
        self.distribution_rules_cache
            .insert(account_id, Some(rules.clone()));

        Ok(rules)
    }

    /// Execute profit distribution for an account using persisted rules.
    pub fn execute_distribution(
        &mut self,
        source_account_id: Uuid,
        profit_amount: Decimal,
        currency: Currency,
    ) -> Result<DistributionResult, Box<dyn std::error::Error>> {
        self.consume_protected_authorization("execute_distribution")?;
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

    /// Returns persisted distribution rules for an account.
    pub fn distribution_rules_for_account(
        &mut self,
        account_id: Uuid,
    ) -> Result<DistributionRules, Box<dyn std::error::Error>> {
        self.factory.distribution_read().for_account(account_id)
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

    /// Configure advisory thresholds for an account.
    pub fn configure_advisory_thresholds(
        &mut self,
        account_id: Uuid,
        thresholds: AdvisoryThresholds,
    ) -> Result<(), Box<dyn std::error::Error>> {
        self.consume_protected_authorization("configure_advisory_thresholds")?;
        thresholds.validate()?;
        self.factory.advisory_write().upsert_advisory_thresholds(
            account_id,
            thresholds.sector_limit_pct,
            thresholds.asset_class_limit_pct,
            thresholds.single_position_limit_pct,
        )?;
        Ok(())
    }

    /// Run advisory checks for a proposed trade and store a history event.
    pub fn advisory_check_trade(
        &mut self,
        proposal: TradeProposal,
    ) -> Result<AdvisoryResult, Box<dyn std::error::Error>> {
        let thresholds = self.advisory_thresholds_for_account(proposal.account_id)?;
        let open = self.open_trades_for_account(proposal.account_id);
        let result = services::advisory::analyze_trade_proposal(&open, &proposal, &thresholds);
        self.advisory_history.push(AdvisoryHistoryEntry {
            account_id: proposal.account_id,
            symbol: proposal.symbol.clone(),
            level: result.level.clone(),
            summary: if result.warnings.is_empty() {
                "ok".to_string()
            } else {
                result.warnings.join("; ")
            },
            created_at: chrono::Utc::now().naive_utc(),
        });
        Ok(result)
    }

    /// Return advisory thresholds for an account, falling back to defaults when unset.
    pub fn advisory_thresholds(
        &mut self,
        account_id: Uuid,
    ) -> Result<AdvisoryThresholds, Box<dyn std::error::Error>> {
        self.advisory_thresholds_for_account(account_id)
    }

    /// Return current portfolio advisory status.
    pub fn advisory_status_for_account(
        &mut self,
        account_id: Uuid,
    ) -> Result<PortfolioAdvisoryStatus, Box<dyn std::error::Error>> {
        let thresholds = self.advisory_thresholds_for_account(account_id)?;
        let open = self.open_trades_for_account(account_id);
        Ok(services::advisory::portfolio_status(&open, &thresholds))
    }

    /// Return in-memory advisory history entries for an account.
    pub fn advisory_history_for_account(
        &self,
        account_id: Uuid,
        days: u32,
    ) -> Vec<AdvisoryHistoryEntry> {
        let cutoff = chrono::Utc::now()
            .naive_utc()
            .checked_sub_signed(chrono::Duration::days(i64::from(days)))
            .unwrap_or_else(|| chrono::Utc::now().naive_utc());
        self.advisory_history
            .iter()
            .filter(|entry| entry.account_id == account_id && entry.created_at >= cutoff)
            .cloned()
            .collect()
    }

    fn advisory_thresholds_for_account(
        &mut self,
        account_id: Uuid,
    ) -> Result<AdvisoryThresholds, Box<dyn std::error::Error>> {
        let thresholds = self
            .factory
            .advisory_read()
            .advisory_thresholds_for_account(account_id)?;

        Ok(match thresholds {
            Some((sector_limit_pct, asset_class_limit_pct, single_position_limit_pct)) => {
                AdvisoryThresholds {
                    sector_limit_pct,
                    asset_class_limit_pct,
                    single_position_limit_pct,
                }
            }
            None => AdvisoryThresholds::default(),
        })
    }
}

impl TrustFacade {
    fn open_trades_for_account(&mut self, account_id: Uuid) -> Vec<Trade> {
        let mut seen = std::collections::HashSet::new();
        let mut out = Vec::new();
        for status in [Status::Submitted, Status::Filled, Status::PartiallyFilled] {
            if let Ok(trades) = self.search_trades(account_id, status) {
                for trade in trades {
                    if seen.insert(trade.id) {
                        out.push(trade);
                    }
                }
            }
        }
        out
    }

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
        .map_err(|e| std::io::Error::other(e.to_string()))?;
    Ok(hash.to_string())
}

fn verify_distribution_password(
    password: &str,
    stored_hash: &str,
) -> Result<bool, Box<dyn std::error::Error>> {
    if stored_hash.starts_with("$argon2") {
        let parsed = PasswordHash::new(stored_hash)
            .map_err(|e| std::io::Error::other(format!("Invalid password hash: {e}")))?;
        Ok(Argon2::default()
            .verify_password(password.as_bytes(), &parsed)
            .is_ok())
    } else {
        Ok(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Duration, TimeZone, Utc};
    use model::OrderIds;

    struct NoopBroker;

    impl Broker for NoopBroker {
        fn kind(&self) -> BrokerKind {
            BrokerKind::Alpaca
        }

        fn submit_trade(
            &self,
            _trade: &Trade,
            _account: &Account,
        ) -> Result<(BrokerLog, OrderIds), Box<dyn StdError>> {
            Err("submit not used in facade wrapper tests".into())
        }

        fn sync_trade(
            &self,
            _trade: &Trade,
            _account: &Account,
        ) -> Result<(Status, Vec<Order>, BrokerLog), Box<dyn StdError>> {
            Err("sync not used in facade wrapper tests".into())
        }

        fn close_trade(
            &self,
            _trade: &Trade,
            _account: &Account,
        ) -> Result<(Order, BrokerLog), Box<dyn StdError>> {
            Err("close not used in facade wrapper tests".into())
        }

        fn cancel_trade(
            &self,
            _trade: &Trade,
            _account: &Account,
        ) -> Result<(), Box<dyn StdError>> {
            Err("cancel not used in facade wrapper tests".into())
        }

        fn modify_stop(
            &self,
            _trade: &Trade,
            _account: &Account,
            _new_stop_price: Decimal,
        ) -> Result<String, Box<dyn StdError>> {
            Err("modify stop not used in facade wrapper tests".into())
        }

        fn modify_target(
            &self,
            _trade: &Trade,
            _account: &Account,
            _new_price: Decimal,
        ) -> Result<String, Box<dyn StdError>> {
            Err("modify target not used in facade wrapper tests".into())
        }

        fn get_latest_quote(
            &self,
            symbol: &str,
            _account: &Account,
        ) -> Result<model::MarketQuote, Box<dyn StdError>> {
            Ok(model::MarketQuote {
                symbol: symbol.to_string(),
                as_of: Utc.with_ymd_and_hms(2024, 1, 1, 9, 30, 0).unwrap(),
                bid_price: dec!(102),
                bid_size: 10,
                ask_price: dec!(99),
                ask_size: 12,
            })
        }

        fn get_latest_trade(
            &self,
            symbol: &str,
            _account: &Account,
        ) -> Result<model::MarketTradeTick, Box<dyn StdError>> {
            Ok(model::MarketTradeTick {
                symbol: symbol.to_string(),
                as_of: Utc.with_ymd_and_hms(2024, 1, 1, 9, 31, 0).unwrap(),
                price: dec!(100),
                size: 7,
            })
        }
    }

    fn new_test_facade() -> TrustFacade {
        TrustFacade::new(
            Box::new(db_sqlite::SqliteDatabase::new_in_memory()),
            Box::new(NoopBroker),
        )
    }

    fn persist_closed_trade_through_facade(trust: &mut TrustFacade) -> (Account, Trade) {
        let account = trust
            .create_account(
                "facade-grade-account",
                "facade grade",
                Environment::Paper,
                dec!(20),
                dec!(10),
            )
            .expect("account should be created");
        trust
            .create_transaction(
                &account,
                &TransactionCategory::Deposit,
                dec!(50000),
                &Currency::USD,
            )
            .expect("deposit should fund account");
        let vehicle = trust
            .create_trading_vehicle(
                "FACGRADE",
                Some("US0000000001"),
                &TradingVehicleCategory::Stock,
                "alpaca",
            )
            .expect("vehicle should be created");
        let trade = trust
            .create_trade(
                DraftTrade {
                    account: account.clone(),
                    trading_vehicle: vehicle,
                    quantity: 10.into(),
                    currency: Currency::USD,
                    category: model::TradeCategory::Long,
                    thesis: Some("Breakout after consolidation".to_string()),
                    sector: Some("Technology".to_string()),
                    asset_class: Some("Stock".to_string()),
                    context: Some("Daily range expansion".to_string()),
                },
                dec!(95),
                dec!(100),
                dec!(115),
            )
            .expect("trade should be created");
        let (mut funded, _, _, _) = trust.fund_trade(&trade).expect("trade should fund");
        funded.entry.average_filled_price = Some(funded.entry.unit_price);
        funded.entry.filled_quantity = funded.entry.quantity;
        trust
            .factory
            .order_write()
            .update(&funded.entry)
            .expect("entry average fill should be persisted");

        let (mut filled, _) = trust
            .fill_trade(&funded, Decimal::ZERO)
            .expect("trade should fill");
        filled.target.average_filled_price = Some(filled.target.unit_price);
        filled.target.filled_quantity = filled.target.quantity;
        trust
            .factory
            .order_write()
            .update(&filled.target)
            .expect("target average fill should be persisted");

        trust
            .target_acquired(&filled, Decimal::ZERO)
            .expect("trade should close at target");
        let mut closed = trust
            .search_trades(account.id, Status::ClosedTarget)
            .expect("closed target trades should be readable");
        let trade = closed.pop().expect("closed trade should exist");

        (account, trade)
    }

    fn create_distribution_child_accounts(trust: &mut TrustFacade, source: &Account) {
        for (suffix, account_type) in [
            ("earnings", AccountType::Earnings),
            ("tax", AccountType::TaxReserve),
            ("reinvestment", AccountType::Reinvestment),
        ] {
            trust
                .create_account_with_profile(
                    &format!("facade-distribution-{suffix}"),
                    &format!("distribution {suffix}"),
                    Environment::Paper,
                    dec!(0),
                    dec!(0),
                    account_type,
                    Some(source.id),
                    BrokerKind::Alpaca,
                    None,
                )
                .expect("distribution child account should be created");
        }
    }

    #[test]
    fn level_snapshot_cache_prunes_recomputes_and_tracks_ordered_wins() {
        let base = Utc::now().naive_utc();
        let outside_window_days =
            LevelingService::<DefaultLevelTransitionPolicy>::EVALUATION_WINDOW_DAYS
                .checked_add(1)
                .unwrap();
        let stale_loss = base
            .checked_sub_signed(Duration::days(outside_window_days))
            .unwrap();
        let recent_win = base.checked_sub_signed(Duration::days(1)).unwrap();
        let out_of_order_loss = recent_win.checked_sub_signed(Duration::hours(1)).unwrap();

        let mut cache = LevelSnapshotCache::new();
        cache.seed_from_points(vec![(stale_loss, dec!(-20)), (recent_win, dec!(10))]);

        let seeded = cache.snapshot(dec!(1000));
        assert_eq!(seeded.profitable_trades, 1);
        assert_eq!(seeded.consecutive_wins, 1);
        assert_eq!(seeded.monthly_loss_percentage, dec!(-1));
        assert_eq!(seeded.largest_loss_percentage, dec!(-2));

        cache.push_and_prune(base, dec!(15));
        let pruned = cache.snapshot(dec!(1000));
        assert_eq!(pruned.profitable_trades, 2);
        assert_eq!(pruned.consecutive_wins, 2);
        assert_eq!(pruned.monthly_loss_percentage, Decimal::ZERO);
        assert_eq!(pruned.largest_loss_percentage, Decimal::ZERO);

        cache.push_and_prune(out_of_order_loss, dec!(-5));
        let recomputed = cache.snapshot(dec!(1000));
        assert_eq!(recomputed.profitable_trades, 2);
        assert_eq!(recomputed.consecutive_wins, 0);
        assert_eq!(recomputed.monthly_loss_percentage, Decimal::ZERO);
        assert_eq!(recomputed.largest_loss_percentage, dec!(-0.5));
    }

    #[test]
    fn level_snapshot_cache_covers_empty_and_lazy_min_recompute_paths() {
        let base = Utc::now().naive_utc();
        let outside_window_days =
            LevelingService::<DefaultLevelTransitionPolicy>::EVALUATION_WINDOW_DAYS
                .checked_add(1)
                .unwrap();
        let stale = base
            .checked_sub_signed(Duration::days(outside_window_days))
            .unwrap();
        let recent = base.checked_sub_signed(Duration::days(1)).unwrap();
        let recent_later = recent.checked_add_signed(Duration::minutes(1)).unwrap();

        let empty = LevelSnapshotCache::new().snapshot(dec!(1000));
        assert_eq!(empty.profitable_trades, 0);
        assert_eq!(empty.win_rate_percentage, Decimal::ZERO);
        assert_eq!(empty.largest_loss_percentage, Decimal::ZERO);

        let mut decreasing_min = LevelSnapshotCache::new();
        decreasing_min.seed_from_points(vec![(recent, dec!(10)), (recent_later, dec!(-20))]);
        assert_eq!(
            decreasing_min.snapshot(dec!(1000)).largest_loss_percentage,
            dec!(-2)
        );

        let mut stale_positive = LevelSnapshotCache::new();
        stale_positive.seed_from_points(vec![(stale, dec!(20)), (recent, dec!(-10))]);
        stale_positive.push_and_prune(base, dec!(5));
        let pruned = stale_positive.snapshot(dec!(1000));
        assert_eq!(pruned.profitable_trades, 1);
        assert_eq!(pruned.largest_loss_percentage, dec!(-1));

        let mut stale_min = LevelSnapshotCache::new();
        stale_min.seed_from_points(vec![
            (stale, dec!(-30)),
            (recent, dec!(10)),
            (recent_later, dec!(-5)),
        ]);
        stale_min.push_and_prune(base, dec!(1));
        assert_eq!(
            stale_min.snapshot(dec!(1000)).largest_loss_percentage,
            dec!(-0.5)
        );

        let mut stale_min_with_larger_remaining_loss = LevelSnapshotCache::new();
        stale_min_with_larger_remaining_loss.seed_from_points(vec![
            (stale, dec!(-30)),
            (recent, dec!(-10)),
            (recent_later, dec!(-5)),
        ]);
        stale_min_with_larger_remaining_loss.push_and_prune(base, dec!(1));
        assert_eq!(
            stale_min_with_larger_remaining_loss
                .snapshot(dec!(1000))
                .largest_loss_percentage,
            dec!(-1)
        );
    }

    #[test]
    fn distribution_password_hashes_verify_and_reject_weak_inputs() {
        assert!(hash_distribution_password("short").is_err());

        let password = "correct horse battery staple";
        let argon_hash = hash_distribution_password(password).unwrap();
        let second_argon_hash = hash_distribution_password(password).unwrap();
        assert!(argon_hash.starts_with("$argon2"));
        assert!(second_argon_hash.starts_with("$argon2"));
        assert_ne!(argon_hash, second_argon_hash);
        assert!(verify_distribution_password(password, &argon_hash).unwrap());
        assert!(!verify_distribution_password("wrong password", &argon_hash).unwrap());

        let legacy_sha256_hash = "cbe6beb26479b568e48058e254b7c50b8d0fef2bd635a0f024ee3f80d1a7084d";
        assert!(!verify_distribution_password(password, legacy_sha256_hash).unwrap());
        assert!(!verify_distribution_password(password, "$argon2id$invalid").unwrap());
    }

    #[test]
    fn facade_debug_and_rule_search_wrappers_are_stable() {
        let mut trust = new_test_facade();
        let debug = format!("{trust:?}");

        assert!(debug.contains("TrustFacade"));
        assert!(debug.contains("protected_mode"));

        let account = trust
            .create_account(
                "facade-rule-account",
                "rules",
                Environment::Paper,
                dec!(20),
                dec!(10),
            )
            .expect("account should be created");
        let rules = trust
            .search_all_rules(account.id)
            .expect("rule wrapper should read through facade");

        assert!(rules.is_empty());
    }

    #[test]
    fn noop_broker_stub_methods_fail_fast() {
        let broker = NoopBroker;
        let trade = Trade::default();
        let account = Account::default();

        assert_eq!(broker.kind(), BrokerKind::Alpaca);
        assert!(broker.submit_trade(&trade, &account).is_err());
        assert!(broker.sync_trade(&trade, &account).is_err());
        assert!(broker.close_trade(&trade, &account).is_err());
        assert!(broker.cancel_trade(&trade, &account).is_err());
        assert!(broker.modify_stop(&trade, &account, dec!(99)).is_err());
        assert!(broker.modify_target(&trade, &account, dec!(101)).is_err());
    }

    #[test]
    fn facade_market_snapshot_v2_uses_quote_trade_extremes() {
        let mut trust = new_test_facade();
        let account = Account::default();
        let snapshot = trust
            .market_snapshot_v2(&account, "SPY", Utc::now())
            .expect("quote/trade snapshot should succeed");

        assert_eq!(snapshot.symbol, "SPY");
        assert_eq!(snapshot.source, MarketSnapshotSource::QuoteTrade);
        assert_eq!(snapshot.last_price, dec!(100));
        assert_eq!(snapshot.volume, 7);
        assert_eq!(snapshot.open, dec!(100));
        assert_eq!(snapshot.high, dec!(100));
        assert_eq!(snapshot.low, dec!(100));
        assert_eq!(
            snapshot.as_of,
            Utc.with_ymd_and_hms(2024, 1, 1, 9, 31, 0).unwrap()
        );
        assert!(snapshot.quote.is_some());
        assert!(snapshot.trade.is_some());
    }

    #[test]
    fn facade_grade_wrappers_compute_persist_and_read_trade_grades() {
        let mut trust = new_test_facade();
        let (account, trade) = persist_closed_trade_through_facade(&mut trust);

        assert!(trust
            .latest_trade_grade(trade.id)
            .expect("latest grade wrapper should read empty state")
            .is_none());

        let computed = trust
            .compute_trade_grade(
                trade.id,
                services::grading::GradingWeightsPermille::default(),
            )
            .expect("closed trade should compute a grade");
        assert_eq!(computed.trade_id, trade.id);
        assert_eq!(computed.grade.trade_id, trade.id);

        assert!(trust
            .latest_trade_grade(trade.id)
            .expect("compute wrapper should not persist")
            .is_none());

        let persisted = trust
            .grade_trade(
                trade.id,
                services::grading::GradingWeightsPermille::default(),
            )
            .expect("grade wrapper should persist a grade");
        let latest = trust
            .latest_trade_grade(trade.id)
            .expect("latest grade wrapper should read persisted grade")
            .expect("persisted grade should exist");
        let account_grades = trust
            .trade_grades_for_account_days(account.id, 30)
            .expect("account grade wrapper should read grade history");

        assert_eq!(latest.id, persisted.grade.id);
        assert_eq!(account_grades.len(), 1);
        assert_eq!(
            account_grades.first().expect("grade should exist").id,
            latest.id
        );
    }

    #[test]
    fn facade_distribution_configuration_updates_and_auto_distributes_profit() {
        let mut trust = new_test_facade();
        let (source, closed_trade) = persist_closed_trade_through_facade(&mut trust);
        create_distribution_child_accounts(&mut trust, &source);

        trust
            .configure_distribution(
                source.id,
                dec!(0.40),
                dec!(0.30),
                dec!(0.30),
                dec!(100),
                "distribution-pass",
            )
            .expect("initial distribution configuration should persist");

        let wrong_password = trust
            .configure_distribution(
                source.id,
                dec!(0.50),
                dec!(0.25),
                dec!(0.25),
                dec!(100),
                "wrong-password",
            )
            .expect_err("existing distribution rules should require the current password");
        assert!(wrong_password.to_string().contains("Invalid distribution"));

        trust
            .configure_distribution(
                source.id,
                dec!(0.50),
                dec!(0.25),
                dec!(0.25),
                dec!(50),
                "distribution-pass",
            )
            .expect("distribution configuration should update with correct password");

        let mut below_threshold_trade = closed_trade.clone();
        below_threshold_trade.balance.total_performance = dec!(25);
        assert!(trust
            .try_auto_distribute_profit_for_trade(&below_threshold_trade)
            .expect("below-threshold distribution check should succeed")
            .is_none());

        let mut eligible_trade = closed_trade;
        eligible_trade.balance.total_performance = dec!(120);
        trust.distribution_rules_cache.clear();
        let result = trust
            .try_auto_distribute_profit_for_trade(&eligible_trade)
            .expect("eligible profit should auto-distribute")
            .expect("distribution result should be present");

        assert_eq!(result.source_account_id, source.id);
        assert_eq!(result.transactions_created.len(), 3);
    }

    #[test]
    fn facade_configure_advisory_thresholds_persists_custom_limits() {
        let mut trust = new_test_facade();
        let account = trust
            .create_account(
                "facade-advisory-config-account",
                "advisory config",
                Environment::Paper,
                dec!(20),
                dec!(10),
            )
            .expect("account should be created");
        let thresholds = AdvisoryThresholds {
            sector_limit_pct: dec!(45),
            asset_class_limit_pct: dec!(55),
            single_position_limit_pct: dec!(65),
        };

        trust
            .configure_advisory_thresholds(account.id, thresholds.clone())
            .expect("advisory thresholds should persist");

        let persisted = trust
            .advisory_thresholds_for_account(account.id)
            .expect("advisory thresholds should read back");
        assert_eq!(persisted.sector_limit_pct, thresholds.sector_limit_pct);
        assert_eq!(
            persisted.asset_class_limit_pct,
            thresholds.asset_class_limit_pct
        );
        assert_eq!(
            persisted.single_position_limit_pct,
            thresholds.single_position_limit_pct
        );
    }

    #[test]
    fn facade_trading_summary_includes_performance_after_closed_trade() {
        let mut trust = new_test_facade();
        let (account, _trade) = persist_closed_trade_through_facade(&mut trust);

        let summary = trust
            .get_trading_summary(Some(account.id))
            .expect("summary should be computed for account with closed trade");

        let performance = summary
            .performance
            .expect("closed trade should be summarized");
        assert_eq!(summary.account_id, Some(account.id));
        assert_eq!(performance.total_trades, 1);
        assert_eq!(performance.winning_trades, 1);
        assert!(summary.equity > Decimal::ZERO);
    }

    #[test]
    fn facade_advisory_status_reads_open_filled_trades() {
        let mut trust = new_test_facade();
        let account = trust
            .create_account(
                "facade-advisory-account",
                "advisory",
                Environment::Paper,
                dec!(20),
                dec!(10),
            )
            .expect("account should be created");
        trust
            .create_transaction(
                &account,
                &TransactionCategory::Deposit,
                dec!(50000),
                &Currency::USD,
            )
            .expect("deposit should fund account");
        let vehicle = trust
            .create_trading_vehicle(
                "FACADV",
                Some("US0000000002"),
                &TradingVehicleCategory::Stock,
                "alpaca",
            )
            .expect("vehicle should be created");
        let trade = trust
            .create_trade(
                DraftTrade {
                    account: account.clone(),
                    trading_vehicle: vehicle,
                    quantity: 10.into(),
                    currency: Currency::USD,
                    category: model::TradeCategory::Long,
                    thesis: None,
                    sector: Some("Technology".to_string()),
                    asset_class: Some("Stock".to_string()),
                    context: None,
                },
                dec!(95),
                dec!(100),
                dec!(115),
            )
            .expect("trade should be created");
        let (mut funded, _, _, _) = trust.fund_trade(&trade).expect("trade should fund");
        funded.entry.average_filled_price = Some(funded.entry.unit_price);
        funded.entry.filled_quantity = funded.entry.quantity;
        trust
            .factory
            .order_write()
            .update(&funded.entry)
            .expect("entry average fill should be persisted");
        let (filled, _) = trust
            .fill_trade(&funded, Decimal::ZERO)
            .expect("trade should fill");

        assert_eq!(filled.status, Status::Filled);

        let status = trust
            .advisory_status_for_account(account.id)
            .expect("advisory status should include open filled trades");

        assert_eq!(status.top_sector_pct, dec!(100));
        assert_eq!(status.top_asset_class_pct, dec!(100));
        assert_eq!(status.top_position_pct, dec!(100));
        assert_eq!(status.level, services::advisory::AdvisoryAlertLevel::Block);
        assert!(!status.warnings.is_empty());
    }
}

mod broker_registry;
mod calculators_account;
pub mod calculators_advanced_metrics;
pub mod calculators_concentration;
pub mod calculators_drawdown;
pub mod calculators_fixed_income;
pub mod calculators_performance;
pub mod calculators_risk;
mod calculators_trade;
mod commands;
/// Domain events used by core workflows.
pub mod events;
mod mocks;
mod security_tests;
/// Core service layer modules.
pub mod services;
mod validators;
