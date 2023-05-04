use std::fmt;

use chrono::NaiveDateTime;
use uuid::Uuid;

/// Rule entity - represents a rule that can be applied to a trade
/// Rules can be used to limit the risk per trade or per month.
/// Rules are a core functionality of Trust given that they are used to limit the risk.
/// For more information about the rules, please check the documentation about rule names.
#[derive(PartialEq, Debug, Clone)]
pub struct Rule {
    pub id: Uuid,

    // Entity timestamps
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
    pub deleted_at: Option<NaiveDateTime>,

    // Entity fields
    /// The name of the rule
    pub name: RuleName,

    /// The description of the rule
    pub description: String,

    /// The priority of the rule. The higher the priority, the more important the rule is.
    pub priority: u32,

    /// The level of the rule. Depending on the level, the rule will affect differently a trade.
    pub level: RuleLevel,

    /// The account that the rule is associated with.
    pub account_id: Uuid,

    /// If the rule is active or not. If the rule is not active, it will not be applied to any trade.
    pub active: bool,
}

/// RuleName entity - represents the name of a rule
#[derive(PartialEq, Debug, Clone, Copy)]
pub enum RuleName {
    /// The maximum risk per trade defined in percentage
    /// This rule is used to limit the risk per trade
    /// If the risk per trade is higher than the maximum risk per trade, the trade will not be executed.
    /// The risk per trade is calculated as the amount of money that can be lost in the trade.
    /// For example:
    ///
    /// 1. If your account is  50_000, and the maximum risk per trade is 2% of the account, then the maximum risk per trade is 1000.
    /// 2. Buy a stock for 40 and put a stop at 38. This means you'll be risking 2 per share.
    /// 3. If you buy 500 shares, you'll be risking 1000.
    /// 4. In this case the rule will be applied and the trade will be approved.
    ///
    /// As well, this rule can be used to calculate how many shares you can buy.
    RiskPerTrade(u32),

    /// The maximum risk per month defined in percentage
    /// This rule is used to limit the risk per month of an entire account
    /// If the risk per month is higher than the maximum risk per month, all the trades will be rejected until the next month.
    /// The risk per month is calculated as the amount of money that can be lost in the month.
    /// For example:
    ///
    /// 1. If your account is  50_000, and the maximum risk per month is 10% of the account, then the maximum risk per month is 5000.
    /// 2. If you lose 5000 in a month, all the trades will be rejected until the next month.
    ///
    /// It is recommended not to set this rule to more than 6% of the account.
    RiskPerMonth(u32),
}

// Implementations

impl fmt::Display for Rule {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "Name: {}, Description: {}", self.name, self.description)
    }
}

impl fmt::Display for RuleName {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            RuleName::RiskPerTrade(_) => write!(f, "risk_per_trade"),
            RuleName::RiskPerMonth(_) => write!(f, "risk_per_month"),
        }
    }
}

impl RuleName {
    pub fn all() -> Vec<RuleName> {
        vec![RuleName::RiskPerTrade(0), RuleName::RiskPerMonth(0)]
    }
}

impl RuleName {
    pub fn risk(&self) -> u32 {
        match self {
            RuleName::RiskPerTrade(value) => *value,
            RuleName::RiskPerMonth(value) => *value,
        }
    }
}

#[derive(PartialEq, Debug)]
pub struct RuleNameParseError;

impl RuleName {
    pub fn parse(s: &str, risk: u32) -> Result<Self, RuleNameParseError> {
        match s {
            "risk_per_trade" => Ok(RuleName::RiskPerTrade(risk)),
            "risk_per_month" => Ok(RuleName::RiskPerMonth(risk)),
            _ => Err(RuleNameParseError),
        }
    }
}

/// RuleLevel entity - represents the level of a rule
#[derive(PartialEq, Debug, Clone, Copy)]
pub enum RuleLevel {
    /// Just print a message in the logs to warn the user about something
    Advice,

    /// This requires some action from the user to fix the issue
    Warning,

    /// This will stop the trade from being executed
    Error,
}

impl RuleLevel {
    pub fn all() -> Vec<RuleLevel> {
        vec![RuleLevel::Advice, RuleLevel::Warning, RuleLevel::Error]
    }
}

impl fmt::Display for RuleLevel {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            RuleLevel::Advice => write!(f, "advice"),
            RuleLevel::Warning => write!(f, "warning"),
            RuleLevel::Error => write!(f, "error"),
        }
    }
}

#[derive(Debug)]
pub struct RuleLevelParseError;
impl std::str::FromStr for RuleLevel {
    type Err = RuleLevelParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "advice" => Ok(RuleLevel::Advice),
            "warning" => Ok(RuleLevel::Warning),
            "error" => Ok(RuleLevel::Error),
            _ => Err(RuleLevelParseError),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_from_string() {
        let result = RuleName::parse("risk_per_trade", 2);
        assert_eq!(result, Ok(RuleName::RiskPerTrade(2)));
        let result = RuleName::parse("risk_per_month", 2);
        assert_eq!(result, Ok(RuleName::RiskPerMonth(2)));
        let result = RuleName::parse("invalid", 0);
        assert_eq!(result, Err(RuleNameParseError));
    }
}
