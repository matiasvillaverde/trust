use crate::{trade_calculators::RiskCalculator, workers::OverviewWorker};
use model::{AccountOverview, DatabaseFactory, Rule, RuleName, Trade};
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use std::error::Error;
use uuid::Uuid;

type FundingValidationResult = Result<(), Box<FundValidationError>>;

// Validate if trade can be funded by checking account overview, available capital and rules
pub fn can_fund(trade: &Trade, database: &mut dyn DatabaseFactory) -> FundingValidationResult {
    // 1.  Get account overview
    let account = database
        .read_account_db()
        .read_account_id(trade.account_id)
        .unwrap();

    // 2. Calculate account overview based on the given trade currency
    // This calculators uses all the transactions to ensure that the account overview is the latest one
    match OverviewWorker::calculate_account(database, &account, &trade.currency) {
        Ok(overview) => {
            // 3. Validate that there is enough capital available to fund the trade
            validate_enough_capital(trade, &overview)?;
            // 4. Validate the trade against all the applicable rules
            validate_rules(trade, &overview, database)
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

fn validate_enough_capital(trade: &Trade, overview: &AccountOverview) -> FundingValidationResult {
    match overview.total_available >= trade.entry.unit_price * Decimal::from(trade.entry.quantity) {
        true => Ok(()),
        false => Err(Box::new(FundValidationError {
            code: FundValidationErrorCode::NotEnoughFunds,
            message: format!(
                "Not enough funds in account {} for currency {}. Available: {} and you are trying to trade: {}",
                trade.account_id,
                trade.currency,
                overview.total_available,
                trade.entry.unit_price * Decimal::from(trade.entry.quantity)
            ),
        })),
    }
}

fn sorted_rules(account_id: Uuid, database: &mut dyn DatabaseFactory) -> Vec<Rule> {
    let mut rules = database
        .read_rule_db()
        .read_all_rules(account_id)
        .unwrap_or_else(|_| vec![]);
    rules.sort_by(|a, b| a.priority.cmp(&b.priority));
    rules
}

fn validate_rules(
    trade: &Trade,
    account_overview: &AccountOverview,
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
                .unwrap();
            }
            RuleName::RiskPerTrade(risk) => {
                validate_risk_per_trade(
                    trade,
                    account_overview,
                    Decimal::from_f32_retain(risk).unwrap(),
                    risk_per_month,
                )?;
            }
        }
    }

    // If no rule is violated, return Ok
    Ok(())
}

// This function validates a trade based on the given risk parameters and account overview.
// If the trade violates any of the rules, it returns an error.
fn validate_risk_per_trade(
    trade: &Trade,
    account_overview: &AccountOverview,
    risk: Decimal,
    risk_per_month: Decimal,
) -> FundingValidationResult {
    // Check if the risk per month limit has been exceeded.
    if risk_per_month < risk {
        return Err(Box::new(FundValidationError {
            code: FundValidationErrorCode::RiskPerMonthExceeded,
            message: format!(
                "Risk per month exceeded for risk per trade rule, maximum that can be at risk is {}, trade is attempting to risk {}",
                risk_per_month,
                risk,
            ),
        }));
    }

    // Calculate the maximum amount that can be risked based on the available funds and risk percentage.
    let maximum_risk = account_overview.total_available * (risk / dec!(100.0));

    // Calculate the total amount that will be risked in this trade.
    let total_risk = (trade.entry.unit_price - trade.safety_stop.unit_price)
        * Decimal::from(trade.entry.quantity);

    // Check if the risk per trade limit has been exceeded.
    if total_risk > maximum_risk {
        return Err(Box::new(FundValidationError {
            code: FundValidationErrorCode::RiskPerTradeExceeded,
            message: format!(
                "Risk per trade exceeded for risk per trade rule, maximum that can be at risk is {}, trade is attempting to risk {}",
                maximum_risk,
                total_risk,
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
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
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
    use model::Order;
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

        let overview = AccountOverview {
            total_available: Decimal::new(100, 0),
            ..Default::default()
        };

        assert!(validate_enough_capital(&trade, &overview).is_ok());
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

        let overview = AccountOverview {
            total_available: Decimal::new(100, 0),
            ..Default::default()
        };

        let result = validate_enough_capital(&trade, &overview);

        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err().message,
            format!("Not enough funds in account {} for currency USD. Available: 100 and you are trying to trade: 10000", id)
        );
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
        let account_overview = AccountOverview {
            total_available: dec!(100),
            ..Default::default()
        };
        let risk = dec!(5);
        let risk_per_month = dec!(6.2);
        assert!(validate_risk_per_trade(&trade, &account_overview, risk, risk_per_month).is_ok());
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
        let account_overview = AccountOverview {
            total_available: dec!(100),
            ..Default::default()
        };
        let risk = dec!(5);
        let risk_per_month = dec!(4.9);
        assert_eq!(
            validate_risk_per_trade(&trade, &account_overview, risk, risk_per_month),
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
        let account_overview = AccountOverview {
            total_available: dec!(100),
            ..Default::default()
        };
        let risk = dec!(3);
        let risk_per_month = dec!(5.1);
        assert_eq!(
            validate_risk_per_trade(&trade, &account_overview, risk, risk_per_month),
            Err(Box::new(FundValidationError {
                code: FundValidationErrorCode::RiskPerTradeExceeded,
                message: "Risk per trade exceeded for risk per trade rule, maximum that can be at risk is 3.00, trade is attempting to risk 5".to_string(),
            }))
        );
    }
}
