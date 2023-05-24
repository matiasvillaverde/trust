use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use std::error::Error;
use trust_model::{Account, Database, RuleName, Trade};

pub struct RuleValidator;
type RuleValidationResult = Result<(), Box<RuleValidationError>>;

impl RuleValidator {
    pub fn validate_creation(
        account: &Account,
        name: &RuleName,
        database: &mut dyn Database,
    ) -> RuleValidationResult {
        if database.rule_for_account(account.id, name).is_ok() {
            Err(Box::new(RuleValidationError {
                code: RuleValidationErrorCode::RuleAlreadyExistsInAccount,
                message: format!(
                    "Rule with name {} already exists in the selected account",
                    name
                ),
            }))
        } else {
            Ok(())
        }
    }

    pub fn validate_trade(trade: &Trade, database: &mut dyn Database) -> RuleValidationResult {
        let overview = database.read_account_overview_currency(trade.account_id, &trade.currency);
        let overview = match overview {
            Ok(overview) => overview,
            Err(error) => {
                return Err(Box::new(RuleValidationError {
                    code: RuleValidationErrorCode::NotEnoughFunds,
                    message: format!(
                        "Not enough funds in account {} for currency {} with error: {}",
                        trade.account_id, trade.currency, error
                    ),
                }))
            }
        };
        let available = overview.total_available.amount;

        if available < (trade.entry.unit_price.amount * Decimal::from(trade.entry.quantity)) {
            return Err(Box::new(RuleValidationError {
                code: RuleValidationErrorCode::NotEnoughFunds,
                message: format!(
                    "Not enough funds in account {} for currency {}. Available: {} and you are trying to trade: {}",
                    trade.account_id, trade.currency, available, trade.entry.unit_price.amount * Decimal::from(trade.entry.quantity)
                ),
            }));
        }

        // Get rules by priority
        let mut rules = database
            .read_all_rules(trade.account_id)
            .unwrap_or_else(|_| vec![]);
        rules.sort_by(|a, b| a.priority.cmp(&b.priority));

        // match rules by name
        for rule in rules {
            match rule.name {
                RuleName::RiskPerMonth(_risk) => {
                    print!("Risk per month rule not implemented");
                }
                RuleName::RiskPerTrade(risk) => {
                    let risk_per_trade =
                        trade.entry.unit_price.amount - trade.safety_stop.unit_price.amount;
                    let total_risk = risk_per_trade * Decimal::from(trade.entry.quantity);
                    let maximum_risk =
                        available * (Decimal::from_f32_retain(risk).unwrap() / dec!(100.0));

                    if total_risk > maximum_risk {
                        return Err(Box::new(RuleValidationError {
                            code: RuleValidationErrorCode::RiskPerTradeExceeded,
                            message: format!(
                                "Risk per trade exceeded for rule {}, maximum that can be at risk is {}, trade is attempting to risk {}",
                                rule.name,
                                maximum_risk,
                                total_risk,
                            ),
                        }));
                    }
                }
            }
        }

        // If no rule is violated, return Ok
        Ok(())
    }
}

#[derive(Debug, PartialEq)]
pub enum RuleValidationErrorCode {
    RuleAlreadyExistsInAccount,
    RiskPerTradeExceeded,
    NotEnoughFunds,
}

#[derive(Debug)]
pub struct RuleValidationError {
    pub code: RuleValidationErrorCode,
    pub message: String,
}

impl std::fmt::Display for RuleValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "RuleValidationError: {}", self.message)
    }
}

impl Error for RuleValidationError {
    fn description(&self) -> &str {
        &self.message
    }
}
