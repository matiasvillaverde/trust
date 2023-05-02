use chrono::NaiveDateTime;
use uuid::Uuid;

/// Rule entity - represents a rule that can be applied to a trade
/// Rules can be used to limit the risk per trade or per month.
/// Rules are a core functionality of Trust given that they are used to limit the risk.
/// For more information about the rules, please check the documentation about rule names.
#[derive(PartialEq, Debug)]
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
#[derive(PartialEq, Debug)]
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

/// RuleLevel entity - represents the level of a rule
#[derive(PartialEq, Debug)]
pub enum RuleLevel {
    /// Just print a message in the logs to warn the user about something
    Advice,

    /// This requires some action from the user to fix the issue
    Warning,

    /// This will stop the trade from being executed
    Error,
}