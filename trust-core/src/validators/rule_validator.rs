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
        unimplemented!()
    }
}

#[derive(Debug, PartialEq)]
pub enum RuleValidationErrorCode {
    RuleAlreadyExistsInAccount,
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
