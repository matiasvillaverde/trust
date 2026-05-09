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
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
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

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use model::{Rule, RuleLevel};
    use std::io;

    struct RuleReadStub {
        existing_rule: Option<Rule>,
    }

    impl ReadRuleDB for RuleReadStub {
        fn read_all_rules(&mut self, _account_id: uuid::Uuid) -> Result<Vec<Rule>, Box<dyn Error>> {
            Ok(self.existing_rule.iter().cloned().collect())
        }

        fn rule_for_account(
            &mut self,
            _account_id: uuid::Uuid,
            _name: &RuleName,
        ) -> Result<Rule, Box<dyn Error>> {
            self.existing_rule
                .clone()
                .ok_or_else(|| Box::new(io::Error::new(io::ErrorKind::NotFound, "missing")) as _)
        }
    }

    fn rule_for(account: &Account, name: RuleName) -> Rule {
        let now = Utc::now().naive_utc();
        Rule {
            id: uuid::Uuid::new_v4(),
            created_at: now,
            updated_at: now,
            deleted_at: None,
            name,
            description: "risk cap".to_string(),
            priority: 1,
            level: RuleLevel::Error,
            account_id: account.id,
            active: true,
        }
    }

    #[test]
    fn can_create_allows_missing_rule_and_rejects_duplicate_rule() {
        let account = Account::default();
        let rule_name = RuleName::RiskPerTrade(2.0);
        let mut empty_db = RuleReadStub {
            existing_rule: None,
        };

        assert!(empty_db
            .read_all_rules(account.id)
            .expect("empty stub should return an empty rule list")
            .is_empty());
        assert!(can_create(&rule_name, &account, &mut empty_db).is_ok());

        let mut duplicate_db = RuleReadStub {
            existing_rule: Some(rule_for(&account, rule_name)),
        };
        assert_eq!(
            duplicate_db
                .read_all_rules(account.id)
                .expect("duplicate stub should return its seeded rule")
                .len(),
            1
        );
        let error = can_create(&rule_name, &account, &mut duplicate_db).unwrap_err();

        assert_eq!(
            error.code,
            RuleValidationErrorCode::RuleAlreadyExistsInAccount
        );
        assert!(error.message.contains("already exists"));
    }

    #[test]
    #[allow(deprecated)]
    fn rule_validation_error_display_and_description_are_stable() {
        let error = RuleValidationError {
            code: RuleValidationErrorCode::RuleAlreadyExistsInAccount,
            message: "duplicate rule".to_string(),
        };

        assert_eq!(
            error.to_string(),
            "RuleValidationError: duplicate rule, code: RuleAlreadyExistsInAccount"
        );
        assert_eq!(std::error::Error::description(&error), "duplicate rule");
    }
}
