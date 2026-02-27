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
        self.result = Some(
            trust.create_rule(
                &self
                    .account
                    .clone()
                    .expect("Did you forget to setup an account?"),
                &self
                    .name
                    .expect("Did you forget to select the rule name first?"),
                &self
                    .description
                    .clone()
                    .expect("Did you forget to enter a description?"),
                &self.level.expect("Did you forget to enter a level?"),
            ),
        );
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
mod tests {
    use super::{RuleDialogBuilder, RuleRemoveDialogBuilder};
    use crate::dialogs::io::{scripted_push_input, scripted_push_select, scripted_reset};
    use crate::dialogs::DialogIo;
    use alpaca_broker::AlpacaBroker;
    use core::TrustFacade;
    use db_sqlite::SqliteDatabase;
    use model::{Environment, RuleLevel, RuleName};
    use rust_decimal_macros::dec;
    use std::collections::VecDeque;
    use std::io::Error as IoError;
    use uuid::Uuid;

    #[derive(Default)]
    struct ScriptedIo {
        selections: VecDeque<Result<Option<usize>, IoError>>,
        inputs: VecDeque<Result<String, IoError>>,
    }

    impl DialogIo for ScriptedIo {
        fn select_index(
            &mut self,
            _prompt: &str,
            _labels: &[String],
            _default: usize,
        ) -> Result<Option<usize>, IoError> {
            self.selections.pop_front().unwrap_or(Ok(None))
        }

        fn confirm(&mut self, _prompt: &str, _default: bool) -> Result<bool, IoError> {
            Ok(false)
        }

        fn input_text(&mut self, _prompt: &str, _allow_empty: bool) -> Result<String, IoError> {
            self.inputs.pop_front().unwrap_or_else(|| Ok(String::new()))
        }
    }

    fn test_trust() -> TrustFacade {
        let path = std::env::temp_dir().join(format!("trust-test-{}.db", Uuid::new_v4()));
        let db = SqliteDatabase::new(path.to_str().expect("valid temp db path"));
        TrustFacade::new(Box::new(db), Box::<AlpacaBroker>::default())
    }

    #[test]
    fn rule_builders_new_start_empty() {
        let create = RuleDialogBuilder::new();
        assert!(create.name.is_none());
        assert!(create.description.is_none());
        assert!(create.level.is_none());
        assert!(create.account.is_none());
        assert!(create.result.is_none());

        let remove = RuleRemoveDialogBuilder::new();
        assert!(remove.account.is_none());
        assert!(remove.rule_to_remove.is_none());
        assert!(remove.result.is_none());
    }

    #[test]
    fn rule_builders_display_handle_error_results() {
        RuleDialogBuilder {
            name: None,
            description: None,
            level: None,
            account: None,
            result: Some(Err("synthetic failure".into())),
        }
        .display();

        RuleRemoveDialogBuilder {
            account: None,
            rule_to_remove: None,
            result: Some(Err("synthetic failure".into())),
        }
        .display();
    }

    #[test]
    fn rule_name_and_level_with_io_select_values() {
        let mut io = ScriptedIo::default();
        io.selections.push_back(Ok(Some(0)));
        let with_name = RuleDialogBuilder::new().name_with_io(&mut io);
        assert!(with_name.name.is_some());

        let mut io = ScriptedIo::default();
        io.selections.push_back(Ok(Some(0)));
        let with_level = RuleDialogBuilder::new().level_with_io(&mut io);
        assert!(with_level.level.is_some());
    }

    #[test]
    fn rule_name_and_level_with_io_keep_none_when_cancelled() {
        let mut io = ScriptedIo::default();
        io.selections.push_back(Ok(None));
        let with_name = RuleDialogBuilder::new().name_with_io(&mut io);
        assert!(with_name.name.is_none());

        let mut io = ScriptedIo::default();
        io.selections.push_back(Ok(None));
        let with_level = RuleDialogBuilder::new().level_with_io(&mut io);
        assert!(with_level.level.is_none());
    }

    #[test]
    fn rule_description_and_risk_with_io_cover_success_and_validation() {
        let mut io = ScriptedIo::default();
        io.inputs.push_back(Ok("desc".to_string()));
        let builder = RuleDialogBuilder::new().description_with_io(&mut io);
        assert_eq!(builder.description.as_deref(), Some("desc"));

        let mut io = ScriptedIo::default();
        io.inputs.push_back(Ok("5.5".to_string()));
        let builder = RuleDialogBuilder {
            name: Some(RuleName::RiskPerTrade(0.0)),
            description: None,
            level: None,
            account: None,
            result: None,
        }
        .risk_with_io(&mut io);
        assert_eq!(builder.name, Some(RuleName::RiskPerTrade(5.5)));

        let mut io = ScriptedIo::default();
        io.inputs.push_back(Ok("200".to_string()));
        let unchanged = RuleDialogBuilder {
            name: Some(RuleName::RiskPerMonth(1.0)),
            description: None,
            level: None,
            account: None,
            result: None,
        }
        .risk_with_io(&mut io);
        assert_eq!(unchanged.name, Some(RuleName::RiskPerMonth(1.0)));
    }

    #[test]
    fn rule_build_and_display_success_paths() {
        let mut trust = test_trust();
        let account = trust
            .create_account("rule-ok", "desc", Environment::Paper, dec!(20), dec!(10))
            .expect("account should be created");

        let built = RuleDialogBuilder {
            name: Some(RuleName::RiskPerTrade(1.5)),
            description: Some("risk per trade".to_string()),
            level: Some(RuleLevel::Warning),
            account: Some(account.clone()),
            result: None,
        }
        .build(&mut trust);

        let created = built
            .result
            .as_ref()
            .expect("result should be set")
            .as_ref()
            .expect("rule should be created");
        assert!(matches!(created.name, RuleName::RiskPerTrade(_)));
        assert_eq!(created.level, RuleLevel::Warning);
        assert_eq!(created.account_id, account.id);
        assert!(created.active);

        built.display();
    }

    #[test]
    #[should_panic(expected = "Did you forget to setup an account?")]
    fn rule_build_panics_when_account_missing() {
        let mut trust = test_trust();
        let _ = RuleDialogBuilder::new().build(&mut trust);
    }

    #[test]
    #[should_panic(expected = "Did you forget to select the rule name first?")]
    fn rule_build_panics_when_name_missing() {
        let mut trust = test_trust();
        let account = trust
            .create_account(
                "rule-missing-name",
                "desc",
                Environment::Paper,
                dec!(20),
                dec!(10),
            )
            .expect("account should be created");

        let _ = RuleDialogBuilder {
            name: None,
            description: Some("desc".to_string()),
            level: Some(RuleLevel::Advice),
            account: Some(account),
            result: None,
        }
        .build(&mut trust);
    }

    #[test]
    fn rule_remove_select_rule_with_io_and_build_deactivate_rule() {
        let mut trust = test_trust();
        let account = trust
            .create_account(
                "rule-remove",
                "desc",
                Environment::Paper,
                dec!(20),
                dec!(10),
            )
            .expect("account should be created");
        let created = trust
            .create_rule(
                &account,
                &RuleName::RiskPerMonth(2.0),
                "monthly rule",
                &RuleLevel::Error,
            )
            .expect("rule should be created");
        let rules = vec![created.clone()];

        let mut io = ScriptedIo::default();
        io.selections.push_back(Ok(Some(0)));
        let selected = RuleRemoveDialogBuilder::new().select_rule_with_io(&rules, &mut io);
        assert_eq!(selected.rule_to_remove, Some(created.clone()));
        assert!(selected.result.is_none());

        let built = RuleRemoveDialogBuilder {
            account: Some(account),
            rule_to_remove: selected.rule_to_remove.clone(),
            result: None,
        }
        .build(&mut trust);

        let removed = built
            .result
            .as_ref()
            .expect("result should be set")
            .as_ref()
            .expect("rule should be deactivated");
        assert!(!removed.active);
        assert_eq!(removed.id, created.id);

        built.display();
    }

    #[test]
    fn rule_remove_select_rule_with_io_sets_error_on_empty_and_cancel() {
        let mut io = ScriptedIo::default();
        let empty = RuleRemoveDialogBuilder::new().select_rule_with_io(&[], &mut io);
        let empty_err = empty
            .result
            .expect("error should be captured")
            .expect_err("expected list-empty error")
            .to_string();
        assert!(empty_err.contains("No rules found"));

        let mut io = ScriptedIo::default();
        io.selections.push_back(Ok(None));
        let canceled = RuleRemoveDialogBuilder::new().select_rule_with_io(
            &[model::Rule {
                id: Uuid::new_v4(),
                created_at: chrono::Utc::now().naive_utc(),
                updated_at: chrono::Utc::now().naive_utc(),
                deleted_at: None,
                name: RuleName::RiskPerTrade(1.0),
                description: "tmp".to_string(),
                priority: 1,
                level: RuleLevel::Advice,
                account_id: Uuid::new_v4(),
                active: true,
            }],
            &mut io,
        );
        let canceled_err = canceled
            .result
            .expect("error should be captured")
            .expect_err("expected cancel error")
            .to_string();
        assert!(canceled_err.contains("Rule selection was canceled"));
    }

    #[test]
    fn rule_wrapper_methods_use_default_console_io_in_tests() {
        let mut trust = test_trust();
        let account = trust
            .create_account(
                "rule-wrapper",
                "desc",
                Environment::Paper,
                dec!(20),
                dec!(10),
            )
            .expect("account should be created");
        trust
            .create_rule(
                &account,
                &RuleName::RiskPerTrade(1.0),
                "tmp",
                &RuleLevel::Advice,
            )
            .expect("rule");

        scripted_reset();
        scripted_push_select(Ok(Some(0)));
        scripted_push_select(Ok(Some(0)));
        scripted_push_input(Ok("wrapper desc".to_string()));
        scripted_push_input(Ok("2.5".to_string()));
        scripted_push_select(Ok(Some(0)));
        scripted_push_select(Ok(Some(0)));

        let created = RuleDialogBuilder::new()
            .account(&mut trust)
            .name()
            .description()
            .risk()
            .level();
        assert_eq!(
            created.account.as_ref().expect("selected account").id,
            account.id
        );
        assert_eq!(created.description.as_deref(), Some("wrapper desc"));
        assert!(matches!(created.name, Some(RuleName::RiskPerTrade(2.5))));
        assert!(created.level.is_some());

        let removed = RuleRemoveDialogBuilder::new()
            .account(&mut trust)
            .select_rule(&mut trust);
        assert_eq!(
            removed.account.as_ref().expect("selected account").id,
            account.id
        );
        if removed.rule_to_remove.is_none() {
            let error = removed
                .result
                .as_ref()
                .expect("missing rule should capture error")
                .as_ref()
                .expect_err("expected error")
                .to_string();
            assert!(
                error.contains("No rules found") || error.contains("canceled"),
                "unexpected remove wrapper error: {error}"
            );
        }
        scripted_reset();
    }
}
