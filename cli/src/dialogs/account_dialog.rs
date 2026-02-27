//! Account management dialog - UI interaction module
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

use crate::dialogs::{dialog_helpers, ConsoleDialogIo, DialogIo};
use crate::views::{AccountBalanceView, AccountView, RuleView};
use core::TrustFacade;
use model::{Account, Environment};
use rust_decimal::Decimal;

#[allow(dead_code)]
pub struct AccountDialogBuilder {
    name: String,
    description: String,
    environment: Option<Environment>,
    tax_percentage: Option<Decimal>,
    earnings_percentage: Option<Decimal>,
    result: Option<Result<Account, Box<dyn Error>>>,
}

#[allow(dead_code)]
impl AccountDialogBuilder {
    pub fn new() -> Self {
        AccountDialogBuilder {
            name: "".to_string(),
            description: "".to_string(),
            environment: None,
            tax_percentage: None,
            earnings_percentage: None,
            result: None,
        }
    }

    pub fn build(mut self, trust: &mut TrustFacade) -> AccountDialogBuilder {
        self.result = Some(trust.create_account(
            &self.name,
            &self.description,
            self.environment.unwrap(),
            self.tax_percentage.unwrap(),
            self.earnings_percentage.unwrap(),
        ));
        self
    }

    pub fn display(self) {
        match self
            .result
            .expect("No result found, did you forget to call build?")
        {
            Ok(account) => AccountView::display_account(account),
            Err(error) => println!("Error creating account: {error:?}"),
        }
    }

    pub fn name(mut self) -> Self {
        let mut io = ConsoleDialogIo::default();
        self = self.name_with_io(&mut io);
        self
    }

    pub fn name_with_io(mut self, io: &mut dyn DialogIo) -> Self {
        match io.input_text("Name: ", false) {
            Ok(value) => self.name = value,
            Err(error) => println!("Error reading name: {error}"),
        }
        self
    }

    pub fn description(mut self) -> Self {
        let mut io = ConsoleDialogIo::default();
        self = self.description_with_io(&mut io);
        self
    }

    pub fn description_with_io(mut self, io: &mut dyn DialogIo) -> Self {
        match io.input_text("Description: ", false) {
            Ok(value) => self.description = value,
            Err(error) => println!("Error reading description: {error}"),
        }
        self
    }

    pub fn environment(mut self) -> Self {
        let mut io = ConsoleDialogIo::default();
        self = self.environment_with_io(&mut io);
        self
    }

    pub fn environment_with_io(mut self, io: &mut dyn DialogIo) -> Self {
        let available_env = Environment::all();
        let labels: Vec<String> = available_env.iter().map(ToString::to_string).collect();
        match io.select_index("Environment:", &labels, 0) {
            Ok(Some(index)) => self.environment = available_env.get(index).copied(),
            Ok(None) => {}
            Err(error) => println!("Error selecting environment: {error}"),
        }
        self
    }

    pub fn tax_percentage(mut self) -> Self {
        let mut io = ConsoleDialogIo::default();
        self = self.tax_percentage_with_io(&mut io);
        self
    }

    pub fn tax_percentage_with_io(mut self, io: &mut dyn DialogIo) -> Self {
        match io.input_text("Taxes percentage", false) {
            Ok(raw) => match raw.parse::<Decimal>() {
                Ok(value) => self.tax_percentage = Some(value),
                Err(_) => println!("Please enter a valid number."),
            },
            Err(error) => println!("Error reading taxes percentage: {error}"),
        }
        self
    }

    pub fn earnings_percentage(mut self) -> Self {
        let mut io = ConsoleDialogIo::default();
        self = self.earnings_percentage_with_io(&mut io);
        self
    }

    pub fn earnings_percentage_with_io(mut self, io: &mut dyn DialogIo) -> Self {
        match io.input_text("Earning percentage", false) {
            Ok(raw) => match raw.parse::<Decimal>() {
                Ok(value) => self.earnings_percentage = Some(value),
                Err(_) => println!("Please enter a valid number."),
            },
            Err(error) => println!("Error reading earnings percentage: {error}"),
        }
        self
    }
}

pub struct AccountSearchDialog {
    result: Option<Result<Account, Box<dyn Error>>>,
}

impl AccountSearchDialog {
    pub fn new() -> Self {
        AccountSearchDialog { result: None }
    }

    pub fn build(self) -> Result<Account, Box<dyn Error>> {
        self.result
            .expect("No result found, did you forget to call search?")
    }

    pub fn display(self, trust: &mut TrustFacade) {
        match self
            .result
            .expect("No result found, did you forget to call search?")
        {
            Ok(account) => {
                let balances = trust
                    .search_all_balances(account.id)
                    .expect("Error searching account balances");
                let rules = trust
                    .search_all_rules(account.id)
                    .expect("Error searching account rules");
                let name = account.name.clone();
                AccountView::display_account(account);
                if balances.is_empty() {
                    println!("No transactions found");
                } else {
                    println!("Overviews:");
                    AccountBalanceView::display_balances(balances, &name);
                }
                println!();
                println!("Rules:");
                RuleView::display_rules(rules, &name);
            }
            Err(error) => println!("Error searching account: {error:?}"),
        }
    }

    pub fn search(mut self, trust: &mut TrustFacade) -> Self {
        let mut io = ConsoleDialogIo::default();
        self = self.search_with_io(trust, &mut io);
        self
    }

    pub fn search_with_io(mut self, trust: &mut TrustFacade, io: &mut dyn DialogIo) -> Self {
        let accounts = trust.search_all_accounts();
        match accounts {
            Ok(accounts) => {
                match dialog_helpers::select_from_list(
                    io,
                    "Which account do you want to use?",
                    &accounts,
                    "No accounts found, did you forget to create one?",
                    "Account selection was canceled",
                ) {
                    Ok(account) => self.result = Some(Ok(account)),
                    Err(error) => self.result = Some(Err(error)),
                }
            }
            Err(error) => self.result = Some(Err(error)),
        }

        self
    }
}

#[cfg(test)]
mod tests {
    use super::{AccountDialogBuilder, AccountSearchDialog};
    use crate::dialogs::io::{scripted_push_input, scripted_push_select, scripted_reset};
    use crate::dialogs::DialogIo;
    use alpaca_broker::AlpacaBroker;
    use core::TrustFacade;
    use db_sqlite::SqliteDatabase;
    use model::Environment;
    use rust_decimal_macros::dec;
    use std::collections::VecDeque;
    use std::io::{Error as IoError, ErrorKind};

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
        TrustFacade::new(
            Box::new(SqliteDatabase::new_in_memory()),
            Box::<AlpacaBroker>::default(),
        )
    }

    #[test]
    fn account_dialog_builder_new_has_expected_defaults() {
        let builder = AccountDialogBuilder::new();
        assert_eq!(builder.name, "");
        assert_eq!(builder.description, "");
        assert!(builder.environment.is_none());
        assert!(builder.tax_percentage.is_none());
        assert!(builder.earnings_percentage.is_none());
        assert!(builder.result.is_none());
    }

    #[test]
    fn account_search_dialog_new_starts_empty() {
        let dialog = AccountSearchDialog::new();
        assert!(dialog.result.is_none());
    }

    #[test]
    fn account_dialog_display_handles_error_result() {
        let dialog = AccountDialogBuilder {
            name: "x".to_string(),
            description: "y".to_string(),
            environment: None,
            tax_percentage: None,
            earnings_percentage: None,
            result: Some(Err("synthetic failure".into())),
        };
        dialog.display();
    }

    #[test]
    fn account_dialog_build_creates_account_successfully() {
        let mut trust = test_trust();
        let dialog = AccountDialogBuilder {
            name: "acc-a".to_string(),
            description: "desc".to_string(),
            environment: Some(Environment::Paper),
            tax_percentage: Some(dec!(0.2)),
            earnings_percentage: Some(dec!(0.1)),
            result: None,
        }
        .build(&mut trust);

        let account = dialog
            .result
            .expect("result should exist")
            .expect("account should be created");
        assert_eq!(account.name, "acc-a");
    }

    #[test]
    fn account_search_with_io_selects_account() {
        let mut trust = test_trust();
        let created = trust
            .create_account("search-a", "desc", Environment::Paper, dec!(20), dec!(10))
            .expect("account");
        let mut io = ScriptedIo::default();
        io.selections.push_back(Ok(Some(0)));

        let dialog = AccountSearchDialog::new().search_with_io(&mut trust, &mut io);
        let account = dialog.build().expect("selected account should exist");
        assert_eq!(account.id, created.id);
    }

    #[test]
    fn account_search_with_io_handles_cancel_and_io_error() {
        let mut trust = test_trust();
        trust
            .create_account("search-b", "desc", Environment::Paper, dec!(20), dec!(10))
            .expect("account");

        let mut canceled_io = ScriptedIo::default();
        canceled_io.selections.push_back(Ok(None));
        let canceled = AccountSearchDialog::new().search_with_io(&mut trust, &mut canceled_io);
        let canceled_err = canceled.build().expect_err("cancel should surface error");
        assert!(canceled_err.to_string().contains("canceled"));

        let mut failing_io = ScriptedIo::default();
        failing_io
            .selections
            .push_back(Err(IoError::new(ErrorKind::Other, "broken tty")));
        let failed = AccountSearchDialog::new().search_with_io(&mut trust, &mut failing_io);
        let failed_err = failed.build().expect_err("io error should surface");
        let message = failed_err.to_string();
        assert!(
            message.contains("Failed to select")
                || message.contains("broken tty")
                || message.contains("selection")
        );
    }

    #[test]
    fn account_search_with_io_returns_error_when_no_accounts() {
        let mut trust = test_trust();
        let mut io = ScriptedIo::default();
        let dialog = AccountSearchDialog::new().search_with_io(&mut trust, &mut io);
        let error = dialog.build().expect_err("empty list should fail");
        assert!(error.to_string().contains("No accounts found"));
    }

    #[test]
    fn account_builder_with_io_setters_cover_success_and_errors() {
        let mut io = ScriptedIo::default();
        io.inputs.push_back(Ok("acc-io".to_string()));
        io.inputs.push_back(Ok("desc-io".to_string()));
        io.selections.push_back(Ok(Some(0)));
        io.inputs.push_back(Ok("20".to_string()));
        io.inputs.push_back(Ok("10".to_string()));

        let builder = AccountDialogBuilder::new()
            .name_with_io(&mut io)
            .description_with_io(&mut io)
            .environment_with_io(&mut io)
            .tax_percentage_with_io(&mut io)
            .earnings_percentage_with_io(&mut io);

        assert_eq!(builder.name, "acc-io");
        assert_eq!(builder.description, "desc-io");
        assert_eq!(builder.environment, Some(Environment::Paper));
        assert_eq!(builder.tax_percentage, Some(dec!(20)));
        assert_eq!(builder.earnings_percentage, Some(dec!(10)));

        io.inputs.push_back(Ok("bad".to_string()));
        io.inputs
            .push_back(Err(IoError::new(ErrorKind::BrokenPipe, "io failed")));
        io.selections.push_back(Ok(None));
        let unchanged = builder
            .tax_percentage_with_io(&mut io)
            .earnings_percentage_with_io(&mut io)
            .environment_with_io(&mut io);
        assert_eq!(unchanged.tax_percentage, Some(dec!(20)));
        assert_eq!(unchanged.earnings_percentage, Some(dec!(10)));
        assert_eq!(unchanged.environment, Some(Environment::Paper));
    }

    #[test]
    fn account_builder_wrapper_methods_use_default_console_io_in_tests() {
        scripted_reset();
        scripted_push_input(Ok("wrapper-name".to_string()));
        scripted_push_input(Ok("wrapper-desc".to_string()));
        scripted_push_select(Ok(Some(0)));
        scripted_push_input(Ok("21".to_string()));
        scripted_push_input(Ok("11".to_string()));

        let builder = AccountDialogBuilder::new()
            .name()
            .description()
            .environment()
            .tax_percentage()
            .earnings_percentage();

        assert_eq!(builder.name, "wrapper-name");
        assert_eq!(builder.description, "wrapper-desc");
        assert_eq!(builder.environment, Some(Environment::Paper));
        assert_eq!(builder.tax_percentage, Some(dec!(21)));
        assert_eq!(builder.earnings_percentage, Some(dec!(11)));
        scripted_reset();
    }

    #[test]
    fn account_search_wrapper_method_uses_default_console_io_in_tests() {
        let mut trust = test_trust();
        let created = trust
            .create_account(
                "wrapper-search",
                "desc",
                Environment::Paper,
                dec!(20),
                dec!(10),
            )
            .expect("create account");

        scripted_reset();
        scripted_push_select(Ok(Some(0)));
        let selected = AccountSearchDialog::new()
            .search(&mut trust)
            .build()
            .expect("search should select account");
        assert_eq!(selected.id, created.id);
        scripted_reset();
    }
}
