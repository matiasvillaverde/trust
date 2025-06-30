use model::{Account, ReadRuleDB, RuleName};
use std::error::Error;

type RuleValidationResult = Result<(), Box<RuleValidationError>>;

pub fn can_create(
    rule: &RuleName,
    account: &Account,
    database: &mut dyn ReadRuleDB,
) -> RuleValidationResult {
    if database.rule_for_account(account.id, rule).is_ok() {
        Err(Box::new(RuleValidationError {
            code: RuleValidationErrorCode::RuleAlreadyExistsInAccount,
            message: format!("Rule with name {rule} already exists in the selected account"),
        }))
    } else {
        Ok(())
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
        write!(
            f,
            "RuleValidationError: {}, code: {:?}",
            self.message, self.code
        )
    }
}

impl Error for RuleValidationError {
    fn description(&self) -> &str {
        &self.message
    }
}
