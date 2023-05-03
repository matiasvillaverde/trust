use std::error::Error;

use crate::dialogs::AccountSearchDialog;
use dialoguer::{theme::ColorfulTheme, FuzzySelect, Input};
use trust_core::Trust;
use trust_model::{Account, Rule, RuleLevel, RuleName};

pub struct RuleDialogBuilder {
    name: Option<RuleName>,
    description: Option<String>,
    priority: Option<u32>,
    level: Option<RuleLevel>,
    account: Option<Account>,
    result: Option<Result<Rule, Box<dyn Error>>>,
}

impl RuleDialogBuilder {
    pub fn new() -> Self {
        RuleDialogBuilder {
            name: None,
            description: None,
            priority: None,
            level: None,
            account: None,
            result: None,
        }
    }

    pub fn build(mut self, trust: &mut Trust) -> RuleDialogBuilder {
        unimplemented!();
        self
    }

    pub fn display(self) {
        match self
            .result
            .expect("No result found, did you forget to call build?")
        {
            Ok(rule) => unimplemented!(),
            Err(error) => println!("Error creating rule: {:?}", error),
        }
    }

    pub fn account(mut self, trust: &mut Trust) -> Self {
        let account = AccountSearchDialog::new().search(trust).build();
        match account {
            Ok(account) => self.account = Some(account),
            Err(error) => println!("Error searching account: {:?}", error),
        }
        self
    }

    pub fn name(mut self) -> Self {
        println!("For more information about each rule, run: rule <rule-name>");

        let available_rules = RuleName::all(); //TODO: Only show the rules that can be added.

        let selected_rule = FuzzySelect::with_theme(&ColorfulTheme::default())
            .with_prompt("Which rule would you like to add?")
            .items(&available_rules[..])
            .interact()
            .map(|index| available_rules.get(index).unwrap())
            .unwrap();

        self.name = Some(*selected_rule);
        self
    }

    pub fn description(mut self) -> Self {
        self.description = Some(
            Input::with_theme(&ColorfulTheme::default())
                .with_prompt("How would you describe your rule?")
                .validate_with({
                    |input: &String| -> Result<(), &str> {
                        match input.parse::<String>() {
                            Ok(_) => Ok(()),
                            Err(_) => Err("Please enter a valid text."),
                        }
                    }
                })
                .interact_text()
                .unwrap(),
        );
        self
    }

    pub fn risk(mut self) -> Self {
        let name = self
            .name
            .expect("Did you forget to select the rule name first?");

        let message = match name {
            RuleName::RiskPerMonth(_) => "Which is the maximum percentage that your account can loose in one single Month? For example: 6% of an account with $1000, you can maximum loose $60.",
            RuleName::RiskPerTrade(_) => "Which is the maximum percentage that your account can you loose per one single trade? For example: 2% of an account with $1000, in one trade you can maximum loose $20.",
        };

        let risk = Input::with_theme(&ColorfulTheme::default())
            .with_prompt(message)
            .validate_with({
                |input: &String| -> Result<(), &str> {
                    match input.parse::<u32>() {
                        Ok(parsed) => {
                            if parsed > 100 {
                                return Err("Please enter a number below 100%");
                            }
                            Ok(())
                        }
                        Err(_) => Err("Please enter a valid number from 0 to 100."),
                    }
                }
            })
            .interact_text()
            .unwrap()
            .parse::<u32>()
            .unwrap();

        self.name = Some(match name {
            RuleName::RiskPerMonth(_) => RuleName::RiskPerMonth(risk),
            RuleName::RiskPerTrade(_) => RuleName::RiskPerTrade(risk),
        });
        self
    }

    pub fn priority(mut self) -> Self {
        let priority = Input::with_theme(&ColorfulTheme::default())
            .with_prompt("Which priority do you want your rule to have? The rule with higher priority will be applied first.")
            .validate_with({
                |input: &String| -> Result<(), &str> {
                    match input.parse::<u32>() {
                        Ok(parsed) => {
                            if parsed > 1000 {
                                return Err("Please enter a number below 1000");
                            }
                            Ok(())
                        },
                        Err(_) => Err("Please enter a valid number from 0 to 1000."),
                    }
                }
            })
            .interact_text()
            .unwrap()
            .parse::<u32>()
            .unwrap();
        self.priority = Some(priority);
        self
    }

    pub fn level(mut self) -> Self {
        let available_levels = RuleLevel::all();

        let selected_level = FuzzySelect::with_theme(&ColorfulTheme::default())
            .with_prompt("Which rule would you like to add?")
            .items(&available_levels[..])
            .interact()
            .map(|index| available_levels.get(index).unwrap())
            .unwrap();

        self.level = Some(*selected_level);
        self
    }
}