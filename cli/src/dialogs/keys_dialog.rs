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

use crate::dialogs::{AccountSearchDialog, ConsoleDialogIo, DialogIo};
use alpaca_broker::{AlpacaBroker, Keys};
use core::TrustFacade;
use model::{Account, Environment};

fn select_environment(io: &mut dyn DialogIo) -> Option<Environment> {
    let available_env = Environment::all();
    let labels = available_env
        .iter()
        .map(std::string::ToString::to_string)
        .collect::<Vec<_>>();
    io.select_index("Environment:", &labels, 0)
        .ok()
        .flatten()
        .and_then(|index| available_env.get(index).copied())
}

pub struct KeysWriteDialogBuilder {
    url: String,
    key_id: String,
    key_secret: String,
    environment: Option<Environment>,
    account: Option<Account>,
    result: Option<Result<Keys, Box<dyn Error>>>,
}

impl KeysWriteDialogBuilder {
    pub fn new() -> Self {
        KeysWriteDialogBuilder {
            url: "".to_string(),
            key_id: "".to_string(),
            key_secret: "".to_string(),
            environment: None,
            account: None,
            result: None,
        }
    }

    pub fn build(mut self) -> KeysWriteDialogBuilder {
        self.result = Some(AlpacaBroker::setup_keys(
            &self.key_id,
            &self.key_secret,
            &self.url,
            &self
                .environment
                .expect("Did you forget to select an environment?"),
            &self
                .account
                .clone()
                .expect("Did you forget to select an account?"),
        ));
        self
    }

    pub fn display(self) {
        match self
            .result
            .expect("No result found, did you forget to call build?")
        {
            Ok(keys) => println!("Keys created: {:?}", keys.key_id),
            Err(error) => println!("Error creating keys: {error:?}"),
        }
    }

    pub fn environment(mut self) -> Self {
        let mut io = ConsoleDialogIo::default();
        self = self.environment_with_io(&mut io);
        self
    }

    pub fn environment_with_io(mut self, io: &mut dyn DialogIo) -> Self {
        self.environment = select_environment(io);
        self
    }

    pub fn url(mut self) -> Self {
        let mut io = ConsoleDialogIo::default();
        self = self.url_with_io(&mut io);
        self
    }

    pub fn url_with_io(mut self, io: &mut dyn DialogIo) -> Self {
        match io.input_text("Url: ", false) {
            Ok(value) => self.url = value,
            Err(error) => println!("Error reading url: {error}"),
        }
        self
    }

    pub fn key_id(mut self) -> Self {
        let mut io = ConsoleDialogIo::default();
        self = self.key_id_with_io(&mut io);
        self
    }

    pub fn key_id_with_io(mut self, io: &mut dyn DialogIo) -> Self {
        match io.input_text("Key: ", false) {
            Ok(value) => self.key_id = value,
            Err(error) => println!("Error reading key id: {error}"),
        }
        self
    }

    pub fn key_secret(mut self) -> Self {
        let mut io = ConsoleDialogIo::default();
        self = self.key_secret_with_io(&mut io);
        self
    }

    pub fn key_secret_with_io(mut self, io: &mut dyn DialogIo) -> Self {
        match io.input_text("Secret: ", false) {
            Ok(value) => self.key_secret = value,
            Err(error) => println!("Error reading key secret: {error}"),
        }
        self
    }

    pub fn account(mut self, trust: &mut TrustFacade) -> Self {
        let mut io = ConsoleDialogIo::default();
        self = self.account_with_io(trust, &mut io);
        self
    }

    pub fn account_with_io(mut self, trust: &mut TrustFacade, io: &mut dyn DialogIo) -> Self {
        let account = AccountSearchDialog::new().search_with_io(trust, io).build();
        match account {
            Ok(account) => self.account = Some(account),
            Err(error) => println!("Error searching account: {error:?}"),
        }
        self
    }
}

pub struct KeysReadDialogBuilder {
    environment: Option<Environment>,
    account: Option<Account>,
    result: Option<Result<Keys, Box<dyn Error>>>,
}

impl KeysReadDialogBuilder {
    pub fn new() -> Self {
        KeysReadDialogBuilder {
            environment: None,
            account: None,
            result: None,
        }
    }

    pub fn build(mut self) -> KeysReadDialogBuilder {
        self.result = Some(AlpacaBroker::read_keys(
            &self
                .environment
                .expect("Did you forget to select an environment?"),
            &self
                .account
                .clone()
                .expect("Did you forget to select an account?"),
        ));
        self
    }

    pub fn display(self) {
        match self
            .result
            .expect("No result found, did you forget to call build?")
        {
            Ok(keys) => println!("Keys stored: {:?}", keys.key_id),
            Err(error) => println!("Error reading keys: {error:?}"),
        }
    }

    pub fn environment(mut self) -> Self {
        let mut io = ConsoleDialogIo::default();
        self = self.environment_with_io(&mut io);
        self
    }

    pub fn environment_with_io(mut self, io: &mut dyn DialogIo) -> Self {
        self.environment = select_environment(io);
        self
    }

    pub fn account(mut self, trust: &mut TrustFacade) -> Self {
        let mut io = ConsoleDialogIo::default();
        self = self.account_with_io(trust, &mut io);
        self
    }

    pub fn account_with_io(mut self, trust: &mut TrustFacade, io: &mut dyn DialogIo) -> Self {
        let account = AccountSearchDialog::new().search_with_io(trust, io).build();
        match account {
            Ok(account) => self.account = Some(account),
            Err(error) => println!("Error searching account: {error:?}"),
        }
        self
    }
}

pub struct KeysDeleteDialogBuilder {
    environment: Option<Environment>,
    account: Option<Account>,
    result: Option<Result<(), Box<dyn Error>>>,
}

impl KeysDeleteDialogBuilder {
    pub fn new() -> Self {
        KeysDeleteDialogBuilder {
            environment: None,
            account: None,
            result: None,
        }
    }

    pub fn build(mut self) -> KeysDeleteDialogBuilder {
        self.result = Some(AlpacaBroker::delete_keys(
            &self
                .environment
                .expect("Did you forget to select an environment?"),
            &self
                .account
                .clone()
                .expect("Did you forget to select an account?"),
        ));
        self
    }

    pub fn display(self) {
        match self
            .result
            .expect("No result found, did you forget to call build?")
        {
            Ok(_) => println!("Keys deleted"),
            Err(error) => println!("Error deleting keys: {error:?}"),
        }
    }

    pub fn environment(mut self) -> Self {
        let mut io = ConsoleDialogIo::default();
        self = self.environment_with_io(&mut io);
        self
    }

    pub fn environment_with_io(mut self, io: &mut dyn DialogIo) -> Self {
        self.environment = select_environment(io);
        self
    }

    pub fn account(mut self, trust: &mut TrustFacade) -> Self {
        let mut io = ConsoleDialogIo::default();
        self = self.account_with_io(trust, &mut io);
        self
    }

    pub fn account_with_io(mut self, trust: &mut TrustFacade, io: &mut dyn DialogIo) -> Self {
        let account = AccountSearchDialog::new().search_with_io(trust, io).build();
        match account {
            Ok(account) => self.account = Some(account),
            Err(error) => println!("Error searching account: {error:?}"),
        }
        self
    }
}

#[cfg(test)]
mod tests {
    use super::{KeysDeleteDialogBuilder, KeysReadDialogBuilder, KeysWriteDialogBuilder};
    use crate::dialogs::io::{scripted_push_input, scripted_push_select, scripted_reset};
    use crate::dialogs::DialogIo;
    use alpaca_broker::{AlpacaBroker, Keys};
    use core::TrustFacade;
    use db_sqlite::SqliteDatabase;
    use model::Environment;
    use rust_decimal_macros::dec;
    use std::collections::VecDeque;
    use std::io::Error as IoError;

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

    #[test]
    fn keys_builders_new_start_with_empty_state() {
        let write = KeysWriteDialogBuilder::new();
        assert_eq!(write.url, "");
        assert_eq!(write.key_id, "");
        assert_eq!(write.key_secret, "");
        assert!(write.environment.is_none());
        assert!(write.account.is_none());
        assert!(write.result.is_none());

        let read = KeysReadDialogBuilder::new();
        assert!(read.environment.is_none());
        assert!(read.account.is_none());
        assert!(read.result.is_none());

        let delete = KeysDeleteDialogBuilder::new();
        assert!(delete.environment.is_none());
        assert!(delete.account.is_none());
        assert!(delete.result.is_none());
    }

    #[test]
    fn keys_builders_display_handle_error_results() {
        KeysWriteDialogBuilder {
            url: String::new(),
            key_id: String::new(),
            key_secret: String::new(),
            environment: None,
            account: None,
            result: Some(Err("synthetic failure".into())),
        }
        .display();

        KeysReadDialogBuilder {
            environment: None,
            account: None,
            result: Some(Err("synthetic failure".into())),
        }
        .display();

        KeysDeleteDialogBuilder {
            environment: None,
            account: None,
            result: Some(Err("synthetic failure".into())),
        }
        .display();
    }

    #[test]
    fn keys_builders_display_handle_success_results() {
        KeysWriteDialogBuilder {
            url: String::new(),
            key_id: String::new(),
            key_secret: String::new(),
            environment: None,
            account: None,
            result: Some(Ok(Keys::new(
                "id",
                "secret",
                "https://paper-api.alpaca.markets",
            ))),
        }
        .display();

        KeysReadDialogBuilder {
            environment: None,
            account: None,
            result: Some(Ok(Keys::new(
                "id",
                "secret",
                "https://paper-api.alpaca.markets",
            ))),
        }
        .display();

        KeysDeleteDialogBuilder {
            environment: None,
            account: None,
            result: Some(Ok(())),
        }
        .display();
    }

    #[test]
    fn keys_environment_with_io_selects_expected_environment() {
        let mut io = ScriptedIo::default();
        io.selections.push_back(Ok(Some(0)));
        let write = KeysWriteDialogBuilder::new().environment_with_io(&mut io);
        assert_eq!(write.environment, Some(model::Environment::Paper));

        let mut io = ScriptedIo::default();
        io.selections.push_back(Ok(Some(1)));
        let read = KeysReadDialogBuilder::new().environment_with_io(&mut io);
        assert_eq!(read.environment, Some(model::Environment::Live));

        let mut io = ScriptedIo::default();
        io.selections.push_back(Ok(Some(0)));
        let delete = KeysDeleteDialogBuilder::new().environment_with_io(&mut io);
        assert_eq!(delete.environment, Some(model::Environment::Paper));
    }

    #[test]
    fn keys_environment_with_io_handles_cancel() {
        let mut io = ScriptedIo::default();
        io.selections.push_back(Ok(None));
        let write = KeysWriteDialogBuilder::new().environment_with_io(&mut io);
        assert!(write.environment.is_none());
    }

    #[test]
    fn keys_write_input_with_io_sets_values_and_handles_errors() {
        let mut io = ScriptedIo::default();
        io.inputs
            .push_back(Ok("https://paper-api.alpaca.markets".to_string()));
        io.inputs.push_back(Ok("key-id".to_string()));
        io.inputs.push_back(Ok("key-secret".to_string()));
        let builder = KeysWriteDialogBuilder::new()
            .url_with_io(&mut io)
            .key_id_with_io(&mut io)
            .key_secret_with_io(&mut io);
        assert_eq!(builder.url, "https://paper-api.alpaca.markets");
        assert_eq!(builder.key_id, "key-id");
        assert_eq!(builder.key_secret, "key-secret");

        let mut io = ScriptedIo::default();
        io.inputs.push_back(Err(IoError::other("url failed")));
        io.inputs.push_back(Err(IoError::other("id failed")));
        io.inputs.push_back(Err(IoError::other("secret failed")));
        let builder = KeysWriteDialogBuilder::new()
            .url_with_io(&mut io)
            .key_id_with_io(&mut io)
            .key_secret_with_io(&mut io);
        assert_eq!(builder.url, "");
        assert_eq!(builder.key_id, "");
        assert_eq!(builder.key_secret, "");
    }

    #[test]
    fn keys_wrapper_methods_use_default_console_io_in_tests() {
        let mut trust = TrustFacade::new(
            Box::new(SqliteDatabase::new_in_memory()),
            Box::new(AlpacaBroker),
        );
        let account = trust
            .create_account(
                "keys-wrapper-account",
                "desc",
                Environment::Paper,
                dec!(20),
                dec!(10),
            )
            .expect("create account");

        scripted_reset();
        scripted_push_select(Ok(Some(0)));
        scripted_push_input(Ok("https://paper-api.alpaca.markets".to_string()));
        scripted_push_input(Ok("id".to_string()));
        scripted_push_input(Ok("secret".to_string()));
        scripted_push_select(Ok(Some(0)));

        let write = KeysWriteDialogBuilder::new()
            .environment()
            .url()
            .key_id()
            .key_secret()
            .account(&mut trust);

        assert_eq!(write.environment, Some(Environment::Paper));
        assert_eq!(write.url, "https://paper-api.alpaca.markets");
        assert_eq!(write.key_id, "id");
        assert_eq!(write.key_secret, "secret");
        assert_eq!(
            write
                .account
                .as_ref()
                .expect("account should be selected")
                .id,
            account.id
        );
        scripted_reset();
    }

    #[test]
    fn keys_account_selection_handles_error_and_success_paths() {
        let mut trust = TrustFacade::new(
            Box::new(SqliteDatabase::new_in_memory()),
            Box::new(AlpacaBroker),
        );

        let mut io = ScriptedIo::default();
        let write = KeysWriteDialogBuilder::new().account_with_io(&mut trust, &mut io);
        assert!(write.account.is_none(), "missing account should keep None");

        let _ = trust
            .create_account(
                "keys-account",
                "desc",
                Environment::Paper,
                dec!(20),
                dec!(10),
            )
            .expect("create account");

        let mut io = ScriptedIo::default();
        io.selections.push_back(Ok(Some(0)));
        let write = KeysWriteDialogBuilder::new().account_with_io(&mut trust, &mut io);
        assert!(write.account.is_some(), "account should be selected");
    }

    #[test]
    #[should_panic(expected = "Did you forget to select an environment?")]
    fn keys_write_build_panics_when_environment_missing() {
        let _ = KeysWriteDialogBuilder::new().build();
    }

    #[test]
    #[should_panic(expected = "Did you forget to select an environment?")]
    fn keys_read_build_panics_when_environment_missing() {
        let _ = KeysReadDialogBuilder::new().build();
    }

    #[test]
    #[should_panic(expected = "Did you forget to select an environment?")]
    fn keys_delete_build_panics_when_environment_missing() {
        let _ = KeysDeleteDialogBuilder::new().build();
    }
}
