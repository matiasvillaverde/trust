use crate::calculators_trade::{RiskCalculator, TradeCapitalRequired};
use model::{AccountBalance, DatabaseFactory, Rule, RuleName, Trade, TradeCategory};
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use std::error::Error;
use uuid::Uuid;

type FundingValidationResult = Result<(), Box<FundValidationError>>;

// Validate if trade can be funded by checking account balance, available capital and rules
pub fn can_fund(trade: &Trade, database: &mut dyn DatabaseFactory) -> FundingValidationResult {
    // 1. Read the cached account projection for the trade currency.
    // Projection updates are applied incrementally on every write path.
    match database
        .account_balance_read()
        .for_currency(trade.account_id, &trade.currency)
    {
        Ok(balance) => {
            // 2. Validate that there is enough capital available to fund the trade
            validate_enough_capital(trade, &balance)?;
            // 3. Validate the trade against all the applicable rules
            validate_rules(trade, &balance, database)
        }
        Err(e) => {
            // If there is not enough funds in the account for the given currency, return an error
            Err(Box::new(FundValidationError {
                code: FundValidationErrorCode::NotEnoughFunds,
                message: format!(
                    "Not enough funds in account {} for currency {}. Error: {}",
                    trade.account_id, trade.currency, e
                ),
            }))
        }
    }
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

fn sorted_rules(account_id: Uuid, database: &mut dyn DatabaseFactory) -> Vec<Rule> {
    let mut rules = database
        .rule_read()
        .read_all_rules(account_id)
        .unwrap_or_else(|_| vec![]);
    rules.sort_by(|a, b| a.priority.cmp(&b.priority));
    rules
}

fn validate_rules(
    trade: &Trade,
    account_balance: &AccountBalance,
    database: &mut dyn DatabaseFactory,
) -> FundingValidationResult {
    // Get rules by priority
    let rules = sorted_rules(trade.account_id, database);
    let mut risk_per_month = dec!(100.0); // Default to 100% of the available capital

    // Match rules by name
    for rule in rules {
        match rule.name {
            RuleName::RiskPerMonth(risk) => {
                risk_per_month = RiskCalculator::calculate_max_percentage_to_risk_current_month(
                    risk,
                    trade.account_id,
                    &trade.currency,
                    database,
                )
                .map_err(|e| {
                    Box::new(FundValidationError {
                        code: FundValidationErrorCode::NotEnoughFunds,
                        message: format!("Error calculating risk per month: {e}"),
                    })
                })?;
            }
            RuleName::RiskPerTrade(risk) => {
                let risk_decimal = Decimal::from_f32_retain(risk).ok_or_else(|| {
                    Box::new(FundValidationError {
                        code: FundValidationErrorCode::NotEnoughFunds,
                        message: format!("Failed to convert risk {risk} to decimal"),
                    })
                })?;
                validate_risk_per_trade(trade, account_balance, risk_decimal, risk_per_month)?;
            }
        }
    }

    // If no rule is violated, return Ok
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
    let risk_percent = risk.checked_div(dec!(100.0)).ok_or_else(|| {
        Box::new(FundValidationError {
            code: FundValidationErrorCode::NotEnoughFunds,
            message: "Division overflow calculating risk percentage".to_string(),
        })
    })?;
    let maximum_risk = account_balance
        .total_available
        .checked_mul(risk_percent)
        .ok_or_else(|| {
            Box::new(FundValidationError {
                code: FundValidationErrorCode::NotEnoughFunds,
                message: "Multiplication overflow calculating maximum risk".to_string(),
            })
        })?;

    // Calculate the total amount that will be risked in this trade.
    let price_diff = trade
        .entry
        .unit_price
        .checked_sub(trade.safety_stop.unit_price)
        .ok_or_else(|| {
            Box::new(FundValidationError {
                code: FundValidationErrorCode::NotEnoughFunds,
                message: "Subtraction overflow calculating price difference".to_string(),
            })
        })?;
    let total_risk = price_diff
        .checked_mul(Decimal::from(trade.entry.quantity))
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

#[derive(Debug, PartialEq)]
pub struct FundValidationError {
    pub code: FundValidationErrorCode,
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
pub enum FundValidationErrorCode {
    RiskPerTradeExceeded,
    RiskPerMonthExceeded,
    NotEnoughFunds,
}

#[cfg(test)]
mod tests {
    use super::*;
    use model::{Order, TradeCategory};
    use uuid::Uuid;

    #[test]
    fn test_validate_enough_capital_success() {
        let trade = Trade {
            entry: Order {
                unit_price: Decimal::new(10, 0),
                quantity: 5,
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
                quantity: 5,
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
    fn test_validate_enough_capital_short_trade_uses_stop_price() {
        // Given: A short trade with entry at $10 and stop at $15
        let trade = Trade {
            category: TradeCategory::Short,
            entry: Order {
                unit_price: dec!(10),
                quantity: 4,
                ..Default::default()
            },
            safety_stop: Order {
                unit_price: dec!(15),
                quantity: 4,
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
                quantity: 4,
                ..Default::default()
            },
            safety_stop: Order {
                unit_price: dec!(15),
                quantity: 4,
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
    fn test_risk_per_trade_success() {
        let trade = Trade {
            entry: Order {
                unit_price: dec!(10),
                quantity: 5,
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
                quantity: 5,
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
    fn test_risk_per_trade_exceeded() {
        let trade = Trade {
            entry: Order {
                unit_price: dec!(10),
                quantity: 5,
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
}
