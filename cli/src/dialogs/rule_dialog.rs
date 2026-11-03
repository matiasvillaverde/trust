//! UI Dialog Module - User Interaction Code
//!
//! TEMPORARY SAFETY ALLOWANCE: This dialog module contains user interaction code
//! that uses .unwrap() and .expect() for UI input handling. While not ideal,
//! these are less critical than business logic safety violations.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use std::error::Error;

use crate::{
    dialogs::{dialog_helpers, AccountSearchDialog, ConsoleDialogIo, DialogIo},
    views::RuleView,
};
use core::TrustFacade;
use model::{Account, Rule, RuleLevel, RuleName};

pub struct RuleDialogBuilder {
    name: Option<RuleName>,
    description: Option<String>,
    level: Option<RuleLevel>,
    account: Option<Account>,
    result: Option<Result<Rule, Box<dyn Error>>>,
}

impl RuleDialogBuilder {
    pub fn new() -> Self {
        RuleDialogBuilder {
            name: None,
            description: None,
            level: None,
            account: None,
            result: None,
        }
    }

    pub fn build(mut self, trust: &mut TrustFacade) -> RuleDialogBuilder {
        self.result = Some((|| {
            trust.create_rule(
                crate::dialogs::dialog_helpers::require(
                    self.account.as_ref(),
                    std::io::ErrorKind::InvalidInput,
                    "Select an account",
                )?,
                crate::dialogs::dialog_helpers::require(
                    self.name.as_ref(),
                    std::io::ErrorKind::InvalidInput,
                    "Select a rule",
                )?,
                crate::dialogs::dialog_helpers::require(
                    self.description.as_deref(),
                    std::io::ErrorKind::InvalidInput,
                    "Enter a description",
                )?,
                crate::dialogs::dialog_helpers::require(
                    self.level.as_ref(),
                    std::io::ErrorKind::InvalidInput,
                    "Select an enforcement level",
                )?,
            )
        })());
        self
    }

    pub fn display(self) {
        match self
            .result
            .expect("No result found, did you forget to call build?")
        {
            Ok(rule) => RuleView::display_rule(rule, &self.account.unwrap().name),
            Err(error) => println!("Error creating rule: {error:?}"),
        }
    }

    pub fn account(mut self, trust: &mut TrustFacade) -> Self {
        let account = AccountSearchDialog::new().search(trust).build();
        match account {
            Ok(account) => self.account = Some(account),
            Err(error) => println!("Error searching account: {error:?}"),
        }
        self
    }

    pub fn name(mut self) -> Self {
        let mut io = ConsoleDialogIo::default();
        self = self.name_with_io(&mut io);
        self
    }

    pub fn name_with_io(mut self, io: &mut dyn DialogIo) -> Self {
        println!("For more information about each rule, run: rule <rule-name>");

        let available_rules = RuleName::all();
        let labels = available_rules
            .iter()
            .map(std::string::ToString::to_string)
            .collect::<Vec<_>>();

        if let Ok(Some(index)) = io.select_index("Rule:", &labels, 0) {
            self.name = available_rules.get(index).copied();
        }
        self
    }

    pub fn description(mut self) -> Self {
        let mut io = ConsoleDialogIo::default();
        self = self.description_with_io(&mut io);
        self
    }

    pub fn description_with_io(mut self, io: &mut dyn DialogIo) -> Self {
        match io.input_text("Description:", false) {
            Ok(value) => self.description = Some(value),
            Err(error) => println!("Error reading description: {error}"),
        }
        self
    }

    pub fn risk(mut self) -> Self {
        let mut io = ConsoleDialogIo::default();
        self = self.risk_with_io(&mut io);
        self
    }

    pub fn risk_with_io(mut self, io: &mut dyn DialogIo) -> Self {
        let name = self
            .name
            .expect("Did you forget to select the rule name first?");

        let risk = match io.input_text("% of risk", false) {
            Ok(raw) => match raw.parse::<f32>() {
                Ok(parsed) if parsed > 100.0 => {
                    println!("Please enter a number below 100%");
                    return self;
                }
                Ok(parsed) if parsed < 0.0 => {
                    println!("Please enter a number above 0%");
                    return self;
                }
                Ok(parsed) => parsed,
                Err(_) => {
                    println!("Please enter a valid number from 0 to 100.");
                    return self;
                }
            },
            Err(error) => {
                println!("Error reading risk: {error}");
                return self;
            }
        };

        self.name = Some(match name {
            RuleName::RiskPerMonth(_) => RuleName::RiskPerMonth(risk),
            RuleName::RiskPerTrade(_) => RuleName::RiskPerTrade(risk),
        });
        self
    }

    pub fn level(mut self) -> Self {
        let mut io = ConsoleDialogIo::default();
        self = self.level_with_io(&mut io);
        self
    }

    pub fn level_with_io(mut self, io: &mut dyn DialogIo) -> Self {
        let available_levels = RuleLevel::all();
        let labels = available_levels
            .iter()
            .map(std::string::ToString::to_string)
            .collect::<Vec<_>>();

        if let Ok(Some(index)) = io.select_index("Level:", &labels, 0) {
            self.level = available_levels.get(index).copied();
        }
        self
    }
}

pub struct RuleRemoveDialogBuilder {
    account: Option<Account>,
    rule_to_remove: Option<Rule>,
    result: Option<Result<Rule, Box<dyn Error>>>,
}

impl RuleRemoveDialogBuilder {
    pub fn new() -> Self {
        RuleRemoveDialogBuilder {
            result: None,
            rule_to_remove: None,
            account: None,
        }
    }

    pub fn build(mut self, trust: &mut TrustFacade) -> RuleRemoveDialogBuilder {
        let selected_rule = self.rule_to_remove.clone().expect("Select a rule first");
        self.result = Some(trust.deactivate_rule(&selected_rule));
        self
    }

    pub fn display(self) {
        match self
            .result
            .expect("No result found, did you forget to call build?")
        {
            Ok(rule) => RuleView::display_rule(rule, &self.account.unwrap().name),
            Err(error) => println!("Error creating rule: {error:?}"),
        }
    }

    pub fn account(mut self, trust: &mut TrustFacade) -> Self {
        let account = AccountSearchDialog::new().search(trust).build();
        match account {
            Ok(account) => self.account = Some(account),
            Err(error) => println!("Error searching account: {error:?}"),
        }
        self
    }

    pub fn select_rule(mut self, trust: &mut TrustFacade) -> Self {
        let account_id = self.account.clone().expect("Select an account first").id;
        let rules = trust.search_rules(account_id).unwrap_or_else(|error| {
            println!("Error reading rules: {error:?}");
            vec![]
        });

        let mut io = ConsoleDialogIo::default();
        self = self.select_rule_with_io(&rules, &mut io);
        self
    }

    fn select_rule_with_io(mut self, rules: &[Rule], io: &mut dyn DialogIo) -> Self {
        match dialog_helpers::select_from_list(
            io,
            "Rule:",
            rules,
            "No rules found",
            "Rule selection was canceled",
        ) {
            Ok(rule) => self.rule_to_remove = Some(rule),
            Err(error) => self.result = Some(Err(error)),
        }
        self
    }
}

#[cfg(test)]
mod tests;
