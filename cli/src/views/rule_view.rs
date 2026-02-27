use model::Rule;
use tabled::settings::style::Style;
use tabled::Table;
use tabled::Tabled;

#[derive(Tabled)]
pub struct RuleView {
    pub account: String,
    pub name: String,
    pub risk: String,
    pub description: String,
    pub priority: String,
    pub level: String,
    pub active: String,
}

impl RuleView {
    fn new(rule: Rule, account_name: &str) -> RuleView {
        RuleView {
            account: crate::views::uppercase_first(account_name),
            name: rule.name.to_string(),
            risk: format!("{} %", rule.name.risk()),
            description: crate::views::uppercase_first(rule.description.as_str()),
            priority: rule.priority.to_string(),
            level: rule.level.to_string(),
            active: rule.active.to_string(),
        }
    }

    pub fn display_rule(r: Rule, account_name: &str) {
        println!();
        println!("Rule: {}", r.id);
        RuleView::display_rules(vec![r], account_name);
        println!();
    }

    pub fn display_rules(rules: Vec<Rule>, account_name: &str) {
        let views: Vec<RuleView> = rules
            .into_iter()
            .map(|r: Rule| RuleView::new(r, account_name))
            .collect();
        let mut table = Table::new(views);
        table.with(Style::modern());
        println!("{table}");
    }
}

#[cfg(test)]
mod tests {
    use super::RuleView;
    use chrono::Utc;
    use model::{Rule, RuleLevel, RuleName};
    use uuid::Uuid;

    fn sample_rule() -> Rule {
        let now = Utc::now().naive_utc();
        Rule {
            id: Uuid::new_v4(),
            created_at: now,
            updated_at: now,
            deleted_at: None,
            name: RuleName::RiskPerTrade(2.5),
            description: "max risk".to_string(),
            priority: 10,
            level: RuleLevel::Warning,
            account_id: Uuid::new_v4(),
            active: true,
        }
    }

    #[test]
    fn new_formats_rule_fields() {
        let rule = sample_rule();

        let view = RuleView::new(rule, "paper");
        assert_eq!(view.account, "Paper");
        assert_eq!(view.name, "risk_per_trade");
        assert_eq!(view.risk, "2.5 %");
        assert_eq!(view.description, "Max risk");
        assert_eq!(view.priority, "10");
        assert_eq!(view.level, "warning");
        assert_eq!(view.active, "true");
    }

    #[test]
    fn display_rules_runs_for_smoke_coverage() {
        RuleView::display_rules(vec![sample_rule()], "main");
    }
}
