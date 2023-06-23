use trust_model::{Account, DatabaseFactory, Rule, RuleLevel, RuleName};

pub struct RuleWorker;

impl RuleWorker {
    pub fn create_rule(
        database: &mut dyn DatabaseFactory,
        account: &Account,
        rule_name: &RuleName,
        description: &str,
        level: &RuleLevel,
    ) -> Result<Rule, Box<dyn std::error::Error>> {
        crate::validators::rule_validator::can_create(
            rule_name,
            account,
            database.read_rule_db().as_mut(),
        )?;
        database.write_rule_db().create_rule(
            account,
            rule_name,
            description,
            RuleWorker::priority_for(rule_name),
            level,
        )
    }

    /// Returns the priority for a given rule name.
    /// The priority is used to determine the order in which rules are applied.
    /// The lower the number, the higher the priority.
    /// For example, a rule with a priority of 1 will be applied before a rule with a priority of 2.
    /// This is useful for rules that are mutually exclusive.
    ///
    /// For example, a rule that limits the risk per trade and a rule that limits the risk per month.
    /// The risk per trade rule should be applied first, so that the risk per month rule can be applied to the remaining funds.
    ///
    /// If the risk per month rule was applied first, it would limit the risk per trade rule.
    /// The risk per trade rule would then be applied to the remaining funds, which would be less than the total funds.
    /// This would result in a lower risk per trade than expected.
    fn priority_for(name: &RuleName) -> u32 {
        match name {
            RuleName::RiskPerMonth(_) => 1,
            RuleName::RiskPerTrade(_) => 2,
        }
    }
}
