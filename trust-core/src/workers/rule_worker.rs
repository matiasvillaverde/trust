use trust_model::{Account, Database, Rule, RuleLevel, RuleName};

use crate::validators::RuleValidator;

pub struct RuleWorker;

impl RuleWorker {
    pub fn create_rule(
        database: &mut dyn Database,
        account: &Account,
        name: &RuleName,
        description: &str,
        priority: u32,
        level: &RuleLevel,
    ) -> Result<Rule, Box<dyn std::error::Error>> {
        RuleValidator::validate_creation(account, name, database)?;
        database.create_rule(account, name, description, priority, level)
    }
}
