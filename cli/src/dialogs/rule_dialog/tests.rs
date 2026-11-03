#![cfg(test)]
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
fn failed_rule_mutations_report_the_cause() {
    crate::test_support::capture::assert_stdout(
        module_path!(),
        "failed_rule_mutations_report_the_cause",
        || {
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
        },
        |output| {
            assert!(output.contains("synthetic failure"), "{output}");
            assert_eq!(
                output.matches("Error creating rule:").count(),
                2,
                "{output}"
            );
        },
    );
}

#[test]
fn remove_select_rule_continues_after_rule_read_error() {
    let mut trust = TrustFacade::new(
        Box::new(crate::test_support::ReadFailureFactory::rules()),
        Box::<AlpacaBroker>::default(),
    );

    let builder = RuleRemoveDialogBuilder {
        account: Some(model::Account {
            id: Uuid::new_v4(),
            ..model::Account::default()
        }),
        rule_to_remove: None,
        result: None,
    }
    .select_rule(&mut trust);

    let err = builder
        .result
        .expect("empty fallback after read error should set result")
        .expect_err("empty fallback should fail selection");
    assert!(err.to_string().contains("No rules found"));
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
fn rule_selection_helpers_handle_io_errors_and_confirm_stub() {
    let mut io = ScriptedIo::default();
    io.selections.push_back(Err(IoError::other("name failed")));
    let with_name = RuleDialogBuilder::new().name_with_io(&mut io);
    assert!(with_name.name.is_none());

    let mut io = ScriptedIo::default();
    io.selections.push_back(Err(IoError::other("level failed")));
    let with_level = RuleDialogBuilder::new().level_with_io(&mut io);
    assert!(with_level.level.is_none());

    let mut io = ScriptedIo::default();
    assert!(!io.confirm("ignored", true).expect("confirm should succeed"));
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
fn rule_description_and_risk_with_io_cover_error_and_lower_bound_paths() {
    let mut io = ScriptedIo::default();
    io.inputs
        .push_back(Err(IoError::other("description failed")));
    let builder = RuleDialogBuilder::new().description_with_io(&mut io);
    assert!(builder.description.is_none());

    let mut io = ScriptedIo::default();
    io.inputs.push_back(Ok("3.25".to_string()));
    let monthly = RuleDialogBuilder {
        name: Some(RuleName::RiskPerMonth(0.0)),
        description: None,
        level: None,
        account: None,
        result: None,
    }
    .risk_with_io(&mut io);
    assert_eq!(monthly.name, Some(RuleName::RiskPerMonth(3.25)));

    let mut io = ScriptedIo::default();
    io.inputs.push_back(Ok("-1".to_string()));
    let negative = RuleDialogBuilder {
        name: Some(RuleName::RiskPerTrade(1.0)),
        description: None,
        level: None,
        account: None,
        result: None,
    }
    .risk_with_io(&mut io);
    assert_eq!(negative.name, Some(RuleName::RiskPerTrade(1.0)));

    let mut io = ScriptedIo::default();
    io.inputs.push_back(Ok("not-a-risk".to_string()));
    let invalid = RuleDialogBuilder {
        name: Some(RuleName::RiskPerTrade(1.0)),
        description: None,
        level: None,
        account: None,
        result: None,
    }
    .risk_with_io(&mut io);
    assert_eq!(invalid.name, Some(RuleName::RiskPerTrade(1.0)));

    let mut io = ScriptedIo::default();
    io.inputs.push_back(Err(IoError::other("risk failed")));
    let failed = RuleDialogBuilder {
        name: Some(RuleName::RiskPerTrade(1.0)),
        description: None,
        level: None,
        account: None,
        result: None,
    }
    .risk_with_io(&mut io);
    assert_eq!(failed.name, Some(RuleName::RiskPerTrade(1.0)));
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
fn rule_build_rejects_when_account_missing() {
    let mut trust = test_trust();
    let before = trust.search_all_accounts().unwrap();
    let rejected = RuleDialogBuilder::new().build(&mut trust);

    let error = rejected
        .result
        .unwrap()
        .expect_err("incomplete input must be rejected");
    assert_eq!(
        error
            .downcast_ref::<std::io::Error>()
            .map(std::io::Error::kind),
        Some(std::io::ErrorKind::InvalidInput)
    );
    assert_eq!(trust.search_all_accounts().unwrap(), before);
}

#[test]
fn rule_build_rejects_when_name_missing() {
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

    let before = trust.search_all_accounts().unwrap();
    let rejected = RuleDialogBuilder {
        name: None,
        description: Some("desc".to_string()),
        level: Some(RuleLevel::Advice),
        account: Some(account),
        result: None,
    }
    .build(&mut trust);

    let error = rejected
        .result
        .unwrap()
        .expect_err("incomplete input must be rejected");
    assert_eq!(
        error
            .downcast_ref::<std::io::Error>()
            .map(std::io::Error::kind),
        Some(std::io::ErrorKind::InvalidInput)
    );
    assert_eq!(trust.search_all_accounts().unwrap(), before);
}

#[test]
fn rule_build_rejects_when_description_missing() {
    let mut trust = test_trust();
    let account = trust
        .create_account(
            "rule-missing-description",
            "desc",
            Environment::Paper,
            dec!(20),
            dec!(10),
        )
        .expect("account should be created");

    let before = trust.search_all_accounts().unwrap();
    let rejected = RuleDialogBuilder {
        name: Some(RuleName::RiskPerTrade(1.0)),
        description: None,
        level: Some(RuleLevel::Advice),
        account: Some(account),
        result: None,
    }
    .build(&mut trust);

    let error = rejected
        .result
        .unwrap()
        .expect_err("incomplete input must be rejected");
    assert_eq!(
        error
            .downcast_ref::<std::io::Error>()
            .map(std::io::Error::kind),
        Some(std::io::ErrorKind::InvalidInput)
    );
    assert_eq!(trust.search_all_accounts().unwrap(), before);
}

#[test]
fn rule_build_rejects_when_level_missing() {
    let mut trust = test_trust();
    let account = trust
        .create_account(
            "rule-missing-level",
            "desc",
            Environment::Paper,
            dec!(20),
            dec!(10),
        )
        .expect("account should be created");

    let before = trust.search_all_accounts().unwrap();
    let rejected = RuleDialogBuilder {
        name: Some(RuleName::RiskPerTrade(1.0)),
        description: Some("desc".to_string()),
        level: None,
        account: Some(account),
        result: None,
    }
    .build(&mut trust);

    let error = rejected
        .result
        .unwrap()
        .expect_err("incomplete input must be rejected");
    assert_eq!(
        error
            .downcast_ref::<std::io::Error>()
            .map(std::io::Error::kind),
        Some(std::io::ErrorKind::InvalidInput)
    );
    assert_eq!(trust.search_all_accounts().unwrap(), before);
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
fn rule_remove_select_rule_with_io_captures_selection_error() {
    let mut io = ScriptedIo::default();
    io.selections
        .push_back(Err(IoError::other("selection failed")));

    let failed = RuleRemoveDialogBuilder::new().select_rule_with_io(
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

    let error = failed
        .result
        .expect("selection error should be captured")
        .expect_err("expected selection error")
        .to_string();
    assert!(error.contains("selection failed"));
}

#[test]
fn rule_search_wrappers_handle_missing_records() {
    let mut trust = test_trust();
    scripted_reset();

    let create = RuleDialogBuilder::new().account(&mut trust);
    assert!(create.account.is_none());

    let remove = RuleRemoveDialogBuilder::new().account(&mut trust);
    assert!(remove.account.is_none());
    scripted_reset();
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
    assert_eq!(created.name, Some(RuleName::RiskPerTrade(2.5)));
    assert!(created.level.is_some());

    let removed = RuleRemoveDialogBuilder::new()
        .account(&mut trust)
        .select_rule(&mut trust);
    assert_eq!(
        removed.account.as_ref().expect("selected account").id,
        account.id
    );
    assert!(removed.rule_to_remove.is_some());
    scripted_reset();
}
