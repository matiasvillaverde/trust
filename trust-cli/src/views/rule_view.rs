use tabled::settings::style::Style;
use tabled::Table;
use tabled::Tabled;
use trust_model::Rule;

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
        println!("{}", table);
    }
}
