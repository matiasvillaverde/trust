use crate::calculators_trade::{QuantityCalculator, RiskCalculator, TradeCapitalRequired};
#[cfg(test)]
use model::RuleName;
use model::{AccountBalance, DatabaseFactory, Rule, Trade, TradeCategory};
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use std::error::Error;
use uuid::Uuid;

type FundingValidationResult = Result<(), Box<FundValidationError>>;

// Validate if trade can be funded by checking account balance, available capital and rules
pub fn can_fund(trade: &Trade, database: &mut dyn DatabaseFactory) -> FundingValidationResult {
    validate_geometry(trade)?;

    // Read the cached account projection for the trade currency. Every financial
    // write updates it atomically with the ledger.
    match database
        .account_balance_read()
        .for_currency(trade.account_id, &trade.currency)
    {
        Ok(balance) => {
            validate_enough_capital(trade, &balance)?;
            validate_rules(trade, &balance, database)?;
            validate_level_adjusted_quantity(trade, database)
        }
        Err(error) => Err(Box::new(FundValidationError {
            code: FundValidationErrorCode::NotEnoughFunds,
            message: format!(
                "Not enough funds in account {} for currency {}. Error: {error}",
                trade.account_id, trade.currency
            ),
        })),
    }
}

fn validate_geometry(trade: &Trade) -> FundingValidationResult {
    if trade.entry.unit_price <= Decimal::ZERO
        || trade.safety_stop.unit_price <= Decimal::ZERO
        || trade.target.unit_price <= Decimal::ZERO
        || trade.entry.quantity != trade.safety_stop.quantity
        || trade.entry.quantity != trade.target.quantity
        || (trade.category == TradeCategory::Long
            && trade.target.unit_price <= trade.entry.unit_price)
        || (trade.category == TradeCategory::Short
            && trade.target.unit_price >= trade.entry.unit_price)
    {
        return Err(Box::new(FundValidationError {
            code: FundValidationErrorCode::InvalidPriceDifference,
            message: "Funding requires positive bracket prices, matching quantities and a target beyond entry in the trade direction".into(),
        }));
    }

    Ok(())
}

fn submission_error(error: impl std::fmt::Display) -> Box<FundValidationError> {
    Box::new(FundValidationError {
        code: FundValidationErrorCode::NotEnoughFunds,
        message: format!("Cannot verify current capital and risk: {error}"),
    })
}

/// Re-authorize a persisted funded trade immediately before committing submission
/// intent. Restore only its own reservation in the calculation; never write a
/// synthetic refund, double-count risk, or reuse the original funding decision.
pub fn can_submit_funded(
    trade: &Trade,
    database: &mut dyn DatabaseFactory,
) -> FundingValidationResult {
    validate_geometry(trade)?;
    let required = TradeCapitalRequired::calculate(trade).map_err(submission_error)?;
    if trade.status != model::Status::Funded
        || trade.balance.funding != required
        || trade.balance.capital_out_market != required
        || !trade.balance.capital_in_market.is_zero()
        || [&trade.entry, &trade.safety_stop, &trade.target]
            .iter()
            .any(|order| !order.filled_quantity.is_zero() || order.filled_at.is_some())
    {
        return Err(submission_error(
            "Funded reservation is inconsistent; reconcile before submission",
        ));
    }
    let ledger_funding = crate::calculators_trade::TradeCapitalFunded::calculate(
        trade.id,
        database.transaction_read().as_mut(),
    )
    .map_err(submission_error)?;
    if ledger_funding != required {
        return Err(submission_error(
            "Funded reservation does not match the ledger",
        ));
    }
    let available = crate::calculators_account::AccountCapitalAvailable::calculate(
        trade.account_id,
        &trade.currency,
        database.transaction_read().as_mut(),
    )
    .map_err(submission_error)?;
    let mut balance = database
        .account_balance_read()
        .for_currency(trade.account_id, &trade.currency)
        .map_err(submission_error)?;
    if balance.total_available != available {
        return Err(submission_error(
            "Account cash projection does not match the ledger",
        ));
    }
    balance.total_available = available
        .checked_add(required)
        .ok_or_else(|| submission_error("Reserved capital overflow"))?;
    validate_enough_capital(trade, &balance)?;
    let rules = sorted_rules(trade.account_id, database)?;
    let (per_trade, per_month) = RiskCalculator::effective_rules_for_reservation(
        trade.account_id,
        &trade.currency,
        &rules,
        database,
        Some(trade),
    )
    .map_err(submission_error)?;
    let risk = per_trade.unwrap_or(per_month);
    validate_risk_per_trade(trade, &balance, risk, per_month)?;
    let base = QuantityCalculator::quantity_for_risk(
        balance.total_available,
        trade.entry.unit_price.max(trade.safety_stop.unit_price),
        trade.entry.unit_price.min(trade.safety_stop.unit_price),
        risk,
    );
    let level = database
        .level_read()
        .level_for_account(trade.account_id)
        .map_err(submission_error)?;
    let allowed =
        QuantityCalculator::apply_multiplier_to_quantity(base, level.risk_multiplier).min(base);
    if trade.entry.quantity > allowed {
        return Err(Box::new(FundValidationError {
            code: FundValidationErrorCode::LevelAdjustedQuantityExceeded,
            message: format!(
                "Current policy permits quantity {allowed}; cancel and prepare a smaller trade"
            ),
        }));
    }
    Ok(())
}

fn validate_enough_capital(trade: &Trade, balance: &AccountBalance) -> FundingValidationResult {
    let required_capital = TradeCapitalRequired::calculate(trade).map_err(|e| {
        Box::new(FundValidationError {
            code: FundValidationErrorCode::NotEnoughFunds,
            message: format!("Error calculating required capital: {e}"),
        })
    })?;

    if balance.total_available >= required_capital {
        Ok(())
    } else {
        Err(Box::new(FundValidationError {
            code: FundValidationErrorCode::NotEnoughFunds,
            message: format!(
                "Not enough funds in account {} for {} trade in {}. \
                Required: {} (based on {}), Available: {}",
                trade.account_id,
                trade.category,
                trade.currency,
                required_capital,
                match trade.category {
                    TradeCategory::Long => "entry price",
                    TradeCategory::Short => "stop price (full amount needed to close)",
                },
                balance.total_available
            ),
        }))
    }
}

fn sorted_rules(
    account_id: Uuid,
    database: &mut dyn DatabaseFactory,
) -> Result<Vec<Rule>, Box<FundValidationError>> {
    let mut rules = database
        .rule_read()
        .read_all_rules(account_id)
        .map_err(|error| {
            Box::new(FundValidationError {
                code: FundValidationErrorCode::NotEnoughFunds,
                message: format!("Cannot load mandatory risk rules: {error}"),
            })
        })?;
    rules.sort_by_key(|rule| rule.priority);
    Ok(rules)
}

fn validate_rules(
    trade: &Trade,
    account_balance: &AccountBalance,
    database: &mut dyn DatabaseFactory,
) -> FundingValidationResult {
    // Get rules by priority
    let rules = sorted_rules(trade.account_id, database)?;
    let (per_trade, per_month) =
        RiskCalculator::effective_rules(trade.account_id, &trade.currency, &rules, database)
            .map_err(|error| {
                Box::new(FundValidationError {
                    code: FundValidationErrorCode::NotEnoughFunds,
                    message: format!("Error calculating risk per month or per trade: {error}"),
                })
            })?;
    validate_risk_per_trade(
        trade,
        account_balance,
        per_trade.unwrap_or(per_month),
        per_month,
    )
}

fn validate_level_adjusted_quantity(
    trade: &Trade,
    database: &mut dyn DatabaseFactory,
) -> FundingValidationResult {
    let sizing = QuantityCalculator::maximum_quantity_with_level(
        trade.account_id,
        if trade.category == TradeCategory::Short {
            trade.safety_stop.unit_price
        } else {
            trade.entry.unit_price
        },
        if trade.category == TradeCategory::Short {
            trade.entry.unit_price
        } else {
            trade.safety_stop.unit_price
        },
        &trade.currency,
        database,
    )
    .map_err(|error| {
        Box::new(FundValidationError {
            code: FundValidationErrorCode::NotEnoughFunds,
            message: format!("Error calculating level-adjusted quantity: {error}"),
        })
    })?;

    let allowed_quantity = sizing.final_quantity;

    if trade.entry.quantity > allowed_quantity {
        return Err(Box::new(FundValidationError {
            code: FundValidationErrorCode::LevelAdjustedQuantityExceeded,
            message: format!(
                "Trade quantity {} exceeds level-adjusted maximum {} (base {}, multiplier {}x)",
                trade.entry.quantity,
                allowed_quantity,
                sizing.base_quantity,
                sizing.level_multiplier
            ),
        }));
    }

    Ok(())
}

// This function validates a trade based on the given risk parameters and account balance.
// If the trade violates any of the rules, it returns an error.
fn validate_risk_per_trade(
    trade: &Trade,
    account_balance: &AccountBalance,
    risk: Decimal,
    risk_per_month: Decimal,
) -> FundingValidationResult {
    if trade.entry.quantity <= Decimal::ZERO {
        return Err(Box::new(FundValidationError {
            code: FundValidationErrorCode::InvalidQuantity,
            message: format!(
                "Trade quantity must be greater than zero, got {}",
                trade.entry.quantity
            ),
        }));
    }

    // Check if the risk per month limit has been exceeded.
    if risk_per_month < risk {
        return Err(Box::new(FundValidationError {
            code: FundValidationErrorCode::RiskPerMonthExceeded,
            message: format!(
                "Risk per month exceeded for risk per trade rule, maximum that can be at risk is {risk_per_month}, trade is attempting to risk {risk}",
            ),
        }));
    }

    // Calculate the maximum amount that can be risked based on the available funds and risk percentage.
    let maximum_risk = calculate_maximum_risk(account_balance, risk)?;

    // Calculate the total amount that will be risked in this trade.
    let price_diff = calculate_price_diff(trade)?;

    if price_diff <= Decimal::ZERO {
        return Err(Box::new(FundValidationError {
            code: FundValidationErrorCode::InvalidPriceDifference,
            message: format!(
                "Invalid risk setup for {} trade: entry={}, stop={}",
                trade.category, trade.entry.unit_price, trade.safety_stop.unit_price
            ),
        }));
    }

    let total_risk = price_diff
        .checked_mul(trade.entry.quantity)
        .ok_or_else(|| {
            Box::new(FundValidationError {
                code: FundValidationErrorCode::NotEnoughFunds,
                message: "Multiplication overflow calculating total risk".to_string(),
            })
        })?;

    // Check if the risk per trade limit has been exceeded.
    if total_risk > maximum_risk {
        return Err(Box::new(FundValidationError {
            code: FundValidationErrorCode::RiskPerTradeExceeded,
            message: format!(
                "Risk per trade exceeded for risk per trade rule, maximum that can be at risk is {maximum_risk}, trade is attempting to risk {total_risk}",
            ),
        }));
    }

    // If no errors were found, return Ok(())
    Ok(())
}

fn calculate_maximum_risk(
    account_balance: &AccountBalance,
    risk: Decimal,
) -> Result<Decimal, Box<FundValidationError>> {
    let risk_percent = risk.checked_div(dec!(100.0)).ok_or_else(|| {
        Box::new(FundValidationError {
            code: FundValidationErrorCode::NotEnoughFunds,
            message: "Division overflow calculating risk percentage".to_string(),
        })
    })?;
    account_balance
        .total_available
        .checked_mul(risk_percent)
        .ok_or_else(|| {
            Box::new(FundValidationError {
                code: FundValidationErrorCode::NotEnoughFunds,
                message: "Multiplication overflow calculating maximum risk".to_string(),
            })
        })
}

fn calculate_price_diff(trade: &Trade) -> Result<Decimal, Box<FundValidationError>> {
    match trade.category {
        TradeCategory::Long => trade
            .entry
            .unit_price
            .checked_sub(trade.safety_stop.unit_price)
            .ok_or_else(|| {
                Box::new(FundValidationError {
                    code: FundValidationErrorCode::NotEnoughFunds,
                    message: "Subtraction overflow calculating price difference".to_string(),
                })
            }),
        TradeCategory::Short => trade
            .safety_stop
            .unit_price
            .checked_sub(trade.entry.unit_price)
            .ok_or_else(|| {
                Box::new(FundValidationError {
                    code: FundValidationErrorCode::NotEnoughFunds,
                    message: "Subtraction overflow calculating price difference".to_string(),
                })
            }),
    }
}

#[derive(Debug, PartialEq)]
/// Funding validation failure, safe to present to authenticated clients.
pub struct FundValidationError {
    /// Machine-readable reason for rejection.
    pub code: FundValidationErrorCode,
    /// Domain validation detail without broker credentials.
    pub message: String,
}

impl std::fmt::Display for FundValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "FundValidationError: {}", self.message)
    }
}

impl Error for FundValidationError {
    fn description(&self) -> &str {
        &self.message
    }
}
#[derive(Debug, PartialEq)]
/// Funding risk and capital constraints.
pub enum FundValidationErrorCode {
    /// Service trading requires explicit per-trade and monthly policies.
    RiskPolicyMissing,
    /// The proposed risk exceeds the per-trade limit.
    RiskPerTradeExceeded,
    /// Monthly losses leave insufficient risk budget.
    RiskPerMonthExceeded,
    /// Quantity exceeds the current level-adjusted limit.
    LevelAdjustedQuantityExceeded,
    /// Entry and protective stop do not define valid risk.
    InvalidPriceDifference,
    /// Quantity must be positive and representable.
    InvalidQuantity,
    /// Available capital cannot fund the trade.
    NotEnoughFunds,
}

#[cfg(test)]
mod tests {
    use super::*;
    use db_sqlite::SqliteDatabase;
    use model::{
        Account, AccountType, Currency, Environment, Order, RuleLevel, TradeCategory,
        TransactionCategory,
    };
    use uuid::Uuid;

    fn create_test_account(database: &SqliteDatabase, name: &str) -> Account {
        database
            .account_write()
            .create_with_hierarchy(
                name,
                name,
                Environment::Paper,
                dec!(0),
                dec!(0),
                AccountType::Primary,
                None,
            )
            .expect("account should be created")
    }

    fn create_test_balance(database: &SqliteDatabase, account: &Account, available: Decimal) {
        let balance = database
            .account_balance_write()
            .create(account, &Currency::USD)
            .expect("account balance should be created");
        database
            .account_balance_write()
            .update(&balance, available, Decimal::ZERO, available, Decimal::ZERO)
            .expect("account balance should be updated");
    }

    fn create_test_deposit(database: &SqliteDatabase, account: &Account, amount: Decimal) {
        database
            .transaction_write()
            .create_transaction(
                account,
                amount,
                &Currency::USD,
                TransactionCategory::Deposit,
            )
            .expect("deposit should be created");
    }

    fn create_default_level(database: &SqliteDatabase, account: &Account) {
        database
            .level_write()
            .create_default_level(account)
            .expect("default level should be created");
    }

    fn create_test_rule(
        database: &SqliteDatabase,
        account: &Account,
        name: RuleName,
        priority: u32,
    ) {
        database
            .rule_write()
            .create_rule(
                account,
                &name,
                "funding validator test rule",
                priority,
                &RuleLevel::Error,
            )
            .expect("rule should be created");
    }

    fn long_trade(
        account_id: Uuid,
        quantity: u64,
        entry_price: Decimal,
        stop_price: Decimal,
    ) -> Trade {
        Trade {
            account_id,
            currency: Currency::USD,
            category: TradeCategory::Long,
            entry: Order {
                unit_price: entry_price,
                quantity: quantity.into(),
                ..Default::default()
            },
            safety_stop: Order {
                unit_price: stop_price,
                quantity: quantity.into(),
                ..Default::default()
            },
            target: Order {
                unit_price: entry_price.checked_add(dec!(1)).expect("test target price"),
                quantity: quantity.into(),
                ..Default::default()
            },
            ..Default::default()
        }
    }

    #[test]
    fn test_can_fund_accepts_trade_with_balance_and_level_adjusted_capacity() {
        let mut database = SqliteDatabase::new_in_memory();
        let account = create_test_account(&database, "funding-success");
        create_test_balance(&database, &account, dec!(1_000));
        create_test_deposit(&database, &account, dec!(1_000));
        create_default_level(&database, &account);
        let trade = long_trade(account.id, 4, dec!(250), dec!(200));

        assert!(can_fund(&trade, &mut database).is_ok());
    }

    #[test]
    fn test_can_fund_reports_missing_account_balance_projection() {
        let mut database = SqliteDatabase::new_in_memory();
        let account = create_test_account(&database, "funding-missing-balance");
        let trade = long_trade(account.id, 1, dec!(100), dec!(90));

        let error = can_fund(&trade, &mut database).unwrap_err();

        assert_eq!(error.code, FundValidationErrorCode::NotEnoughFunds);
        assert!(error.message.contains("Not enough funds in account"));
        assert!(error.message.contains("currency USD"));
    }

    #[test]
    fn test_validate_enough_capital_success() {
        let trade = Trade {
            entry: Order {
                unit_price: Decimal::new(10, 0),
                quantity: 5.into(),
                ..Default::default()
            },
            ..Default::default()
        };

        let balance = AccountBalance {
            total_available: Decimal::new(100, 0),
            ..Default::default()
        };

        assert!(validate_enough_capital(&trade, &balance).is_ok());
    }

    #[test]
    fn test_validate_enough_capital_failure() {
        let id = Uuid::new_v4();
        let trade = Trade {
            account_id: id,
            entry: Order {
                unit_price: Decimal::new(2000, 0),
                quantity: 5.into(),
                ..Default::default()
            },
            ..Default::default()
        };

        let balance = AccountBalance {
            total_available: Decimal::new(100, 0),
            ..Default::default()
        };

        let result = validate_enough_capital(&trade, &balance);

        assert!(result.is_err());
        let err_msg = result.unwrap_err().message;
        assert!(err_msg.contains("10000")); // Required amount
        assert!(err_msg.contains("100")); // Available amount
    }

    #[test]
    fn test_validate_enough_capital_reports_required_capital_overflow() {
        let trade = Trade {
            entry: Order {
                unit_price: Decimal::MAX,
                quantity: 2.into(),
                ..Default::default()
            },
            ..Default::default()
        };
        let balance = AccountBalance {
            total_available: Decimal::MAX,
            ..Default::default()
        };

        let error = validate_enough_capital(&trade, &balance).unwrap_err();

        assert_eq!(error.code, FundValidationErrorCode::NotEnoughFunds);
        assert!(error.message.contains("Error calculating required capital"));
        assert!(error.message.contains("Arithmetic overflow"));
    }

    #[test]
    fn test_validate_enough_capital_short_trade_uses_stop_price() {
        // Given: A short trade with entry at $10 and stop at $15
        let trade = Trade {
            category: TradeCategory::Short,
            entry: Order {
                unit_price: dec!(10),
                quantity: 4.into(),
                ..Default::default()
            },
            safety_stop: Order {
                unit_price: dec!(15),
                quantity: 4.into(),
                ..Default::default()
            },
            ..Default::default()
        };

        // When: Validating with balance of $60 (enough for stop: 15*4=60)
        let balance = AccountBalance {
            total_available: dec!(60),
            ..Default::default()
        };

        // Then: Should pass validation
        assert!(validate_enough_capital(&trade, &balance).is_ok());
    }

    #[test]
    fn test_validate_enough_capital_short_trade_insufficient_for_stop() {
        // Given: A short trade with entry at $10 and stop at $15
        let id = Uuid::new_v4();
        let trade = Trade {
            account_id: id,
            category: TradeCategory::Short,
            entry: Order {
                unit_price: dec!(10),
                quantity: 4.into(),
                ..Default::default()
            },
            safety_stop: Order {
                unit_price: dec!(15),
                quantity: 4.into(),
                ..Default::default()
            },
            ..Default::default()
        };

        // When: Validating with balance of $45 (not enough for stop: 15*4=60)
        let balance = AccountBalance {
            total_available: dec!(45),
            ..Default::default()
        };

        // Then: Should fail with clear error message
        let result = validate_enough_capital(&trade, &balance);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.message.contains("stop price"));
        assert!(err.message.contains("60")); // Required amount
        assert!(err.message.contains("45")); // Available amount
    }

    #[test]
    fn test_validate_rules_rejects_zero_ledger_capital_even_with_stale_projection() {
        let mut database = SqliteDatabase::new_in_memory();
        let account = create_test_account(&database, "funding-risk-month-error");
        create_test_rule(&database, &account, RuleName::RiskPerMonth(2.0), 1);
        let trade = long_trade(account.id, 1, dec!(100), dec!(90));
        let account_balance = AccountBalance {
            account_id: account.id,
            total_available: dec!(1_000),
            currency: Currency::USD,
            ..Default::default()
        };

        let error = validate_rules(&trade, &account_balance, &mut database).unwrap_err();

        assert_eq!(error.code, FundValidationErrorCode::RiskPerTradeExceeded);
        assert!(error.message.contains("maximum that can be at risk is 0"));
    }

    #[test]
    fn test_validate_level_adjusted_quantity_does_not_skip_short_trades() {
        let mut database = SqliteDatabase::new_in_memory();
        let trade = Trade {
            category: TradeCategory::Short,
            entry: Order {
                unit_price: dec!(100),
                quantity: Decimal::from(u64::MAX),
                ..Default::default()
            },
            safety_stop: Order {
                unit_price: dec!(110),
                ..Default::default()
            },
            ..Default::default()
        };

        assert!(validate_level_adjusted_quantity(&trade, &mut database).is_err());
    }

    #[test]
    fn test_validate_level_adjusted_quantity_wraps_calculator_errors() {
        let mut database = SqliteDatabase::new_in_memory();
        let account = create_test_account(&database, "funding-level-calculator-error");
        let trade = long_trade(account.id, 1, Decimal::ZERO, dec!(90));

        let error = validate_level_adjusted_quantity(&trade, &mut database).unwrap_err();

        assert_eq!(error.code, FundValidationErrorCode::NotEnoughFunds);
        assert!(error
            .message
            .contains("Error calculating level-adjusted quantity"));
        assert!(error.message.contains("Invalid entry price"));
    }

    #[test]
    fn test_validate_level_adjusted_quantity_rejects_quantity_above_adjusted_cap() {
        let mut database = SqliteDatabase::new_in_memory();
        let account = create_test_account(&database, "funding-level-cap");
        create_test_deposit(&database, &account, dec!(1_000));
        create_default_level(&database, &account);
        let trade = long_trade(account.id, 5, dec!(250), dec!(200));

        let error = validate_level_adjusted_quantity(&trade, &mut database).unwrap_err();

        assert_eq!(
            error.code,
            FundValidationErrorCode::LevelAdjustedQuantityExceeded
        );
        assert!(error.message.contains("exceeds level-adjusted maximum 4"));
        assert!(error.message.contains("base 4"));
    }

    #[test]
    fn test_risk_per_trade_success() {
        let trade = Trade {
            entry: Order {
                unit_price: dec!(10),
                quantity: 5.into(),
                ..Default::default()
            },
            safety_stop: Order {
                unit_price: dec!(9),
                ..Default::default()
            },
            ..Default::default()
        };
        let account_balance = AccountBalance {
            total_available: dec!(100),
            ..Default::default()
        };
        let risk = dec!(5);
        let risk_per_month = dec!(6.2);
        assert!(validate_risk_per_trade(&trade, &account_balance, risk, risk_per_month).is_ok());
    }

    #[test]
    fn test_risk_per_month_exceeded() {
        let trade = Trade {
            entry: Order {
                unit_price: dec!(10),
                quantity: 5.into(),
                ..Default::default()
            },
            safety_stop: Order {
                unit_price: dec!(9),
                ..Default::default()
            },
            ..Default::default()
        };
        let account_balance = AccountBalance {
            total_available: dec!(100),
            ..Default::default()
        };
        let risk = dec!(5);
        let risk_per_month = dec!(4.9);
        assert_eq!(
            validate_risk_per_trade(&trade, &account_balance, risk, risk_per_month),
            Err(Box::new(FundValidationError {
                code: FundValidationErrorCode::RiskPerMonthExceeded,
                message: "Risk per month exceeded for risk per trade rule, maximum that can be at risk is 4.9, trade is attempting to risk 5".to_string(),
            }))
        );
    }

    #[test]
    fn test_risk_per_trade_entry_equals_stop_rejected() {
        let trade = Trade {
            entry: Order {
                unit_price: dec!(10),
                quantity: 5.into(),
                ..Default::default()
            },
            safety_stop: Order {
                unit_price: dec!(10),
                ..Default::default()
            },
            ..Default::default()
        };
        let account_balance = AccountBalance {
            total_available: dec!(100),
            ..Default::default()
        };
        let risk = dec!(5);
        let risk_per_month = dec!(6.2);
        let result = validate_risk_per_trade(&trade, &account_balance, risk, risk_per_month);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.code, FundValidationErrorCode::InvalidPriceDifference);
    }

    #[test]
    fn test_risk_per_trade_zero_quantity_rejected_before_risk_math() {
        let trade = Trade {
            entry: Order {
                unit_price: dec!(10),
                quantity: 0.into(),
                ..Default::default()
            },
            safety_stop: Order {
                unit_price: dec!(9),
                ..Default::default()
            },
            ..Default::default()
        };
        let account_balance = AccountBalance {
            total_available: dec!(100),
            ..Default::default()
        };

        let error =
            validate_risk_per_trade(&trade, &account_balance, dec!(5), dec!(6)).unwrap_err();

        assert_eq!(error.code, FundValidationErrorCode::InvalidQuantity);
        assert!(error.message.contains("greater than zero"));
    }

    #[test]
    fn test_risk_per_trade_short_stop_below_entry_rejected() {
        let trade = Trade {
            category: TradeCategory::Short,
            entry: Order {
                unit_price: dec!(10),
                quantity: 5.into(),
                ..Default::default()
            },
            safety_stop: Order {
                unit_price: dec!(9),
                ..Default::default()
            },
            ..Default::default()
        };
        let account_balance = AccountBalance {
            total_available: dec!(100),
            ..Default::default()
        };

        let error =
            validate_risk_per_trade(&trade, &account_balance, dec!(5), dec!(6)).unwrap_err();

        assert_eq!(error.code, FundValidationErrorCode::InvalidPriceDifference);
        assert!(error.message.contains("short trade"));
    }

    #[test]
    fn test_risk_per_trade_exceeded() {
        let trade = Trade {
            entry: Order {
                unit_price: dec!(10),
                quantity: 5.into(),
                ..Default::default()
            },
            safety_stop: Order {
                unit_price: dec!(9),
                ..Default::default()
            },
            ..Default::default()
        };
        let account_balance = AccountBalance {
            total_available: dec!(100),
            ..Default::default()
        };
        let risk = dec!(3);
        let risk_per_month = dec!(5.1);
        assert_eq!(
            validate_risk_per_trade(&trade, &account_balance, risk, risk_per_month),
            Err(Box::new(FundValidationError {
                code: FundValidationErrorCode::RiskPerTradeExceeded,
                message: "Risk per trade exceeded for risk per trade rule, maximum that can be at risk is 3.00, trade is attempting to risk 5".to_string(),
            }))
        );
    }

    #[test]
    fn test_risk_per_trade_reports_total_risk_overflow() {
        let trade = Trade {
            entry: Order {
                unit_price: Decimal::MAX,
                quantity: 2.into(),
                ..Default::default()
            },
            safety_stop: Order {
                unit_price: Decimal::ZERO,
                ..Default::default()
            },
            ..Default::default()
        };
        let account_balance = AccountBalance {
            total_available: Decimal::MAX,
            ..Default::default()
        };

        let error =
            validate_risk_per_trade(&trade, &account_balance, dec!(100), dec!(100)).unwrap_err();

        assert_eq!(error.code, FundValidationErrorCode::NotEnoughFunds);
        assert_eq!(
            error.message,
            "Multiplication overflow calculating total risk"
        );
    }

    #[test]
    fn test_risk_per_trade_reports_maximum_risk_overflow() {
        let trade = Trade {
            entry: Order {
                unit_price: dec!(10),
                quantity: 1.into(),
                ..Default::default()
            },
            safety_stop: Order {
                unit_price: dec!(9),
                ..Default::default()
            },
            ..Default::default()
        };
        let account_balance = AccountBalance {
            total_available: Decimal::MAX,
            ..Default::default()
        };

        let error =
            validate_risk_per_trade(&trade, &account_balance, dec!(200), dec!(200)).unwrap_err();

        assert_eq!(error.code, FundValidationErrorCode::NotEnoughFunds);
        assert_eq!(
            error.message,
            "Multiplication overflow calculating maximum risk"
        );
    }

    #[test]
    fn test_calculate_price_diff_reports_long_subtraction_overflow() {
        let trade = Trade {
            category: TradeCategory::Long,
            entry: Order {
                unit_price: Decimal::MIN,
                ..Default::default()
            },
            safety_stop: Order {
                unit_price: Decimal::MAX,
                ..Default::default()
            },
            ..Default::default()
        };

        let error = calculate_price_diff(&trade).unwrap_err();

        assert_eq!(error.code, FundValidationErrorCode::NotEnoughFunds);
        assert_eq!(
            error.message,
            "Subtraction overflow calculating price difference"
        );
    }

    #[test]
    fn test_calculate_price_diff_reports_short_subtraction_overflow() {
        let trade = Trade {
            category: TradeCategory::Short,
            entry: Order {
                unit_price: Decimal::MAX,
                ..Default::default()
            },
            safety_stop: Order {
                unit_price: Decimal::MIN,
                ..Default::default()
            },
            ..Default::default()
        };

        let error = calculate_price_diff(&trade).unwrap_err();

        assert_eq!(error.code, FundValidationErrorCode::NotEnoughFunds);
        assert_eq!(
            error.message,
            "Subtraction overflow calculating price difference"
        );
    }

    #[test]
    #[allow(deprecated)]
    fn fund_validation_error_display_and_description_are_stable() {
        let error = FundValidationError {
            code: FundValidationErrorCode::NotEnoughFunds,
            message: "not enough cash".to_string(),
        };

        assert_eq!(error.to_string(), "FundValidationError: not enough cash");
        assert_eq!(std::error::Error::description(&error), "not enough cash");
    }
}
