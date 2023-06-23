use std::error::Error;

use crate::dialogs::AccountSearchDialog;
use dialoguer::{theme::ColorfulTheme, FuzzySelect, Input};
use alpaca_broker::{AlpacaBroker, Keys};
use core::TrustFacade;
use model::{Account, Environment};

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
            Err(error) => println!("Error creating keys: {:?}", error),
        }
    }

    pub fn environment(mut self) -> Self {
        let available_env = Environment::all();

        let env: &Environment = FuzzySelect::with_theme(&ColorfulTheme::default())
            .with_prompt("Environment:")
            .items(&available_env[..])
            .interact()
            .map(|index| available_env.get(index).unwrap())
            .unwrap();

        self.environment = Some(*env);
        self
    }

    pub fn url(mut self) -> Self {
        self.url = Input::with_theme(&ColorfulTheme::default())
            .with_prompt("Url: ")
            .interact_text()
            .unwrap();
        self
    }

    pub fn key_id(mut self) -> Self {
        self.key_id = Input::with_theme(&ColorfulTheme::default())
            .with_prompt("Key: ")
            .interact_text()
            .unwrap();
        self
    }

    pub fn key_secret(mut self) -> Self {
        self.key_secret = Input::with_theme(&ColorfulTheme::default())
            .with_prompt("Secret: ")
            .interact_text()
            .unwrap();
        self
    }

    pub fn account(mut self, trust: &mut TrustFacade) -> Self {
        let account = AccountSearchDialog::new().search(trust).build();
        match account {
            Ok(account) => self.account = Some(account),
            Err(error) => println!("Error searching account: {:?}", error),
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
            Err(error) => println!("Error reading keys: {:?}", error),
        }
    }

    pub fn environment(mut self) -> Self {
        let available_env = Environment::all();

        let env: &Environment = FuzzySelect::with_theme(&ColorfulTheme::default())
            .with_prompt("Environment:")
            .items(&available_env[..])
            .interact()
            .map(|index| available_env.get(index).unwrap())
            .unwrap();

        self.environment = Some(*env);
        self
    }

    pub fn account(mut self, trust: &mut TrustFacade) -> Self {
        let account = AccountSearchDialog::new().search(trust).build();
        match account {
            Ok(account) => self.account = Some(account),
            Err(error) => println!("Error searching account: {:?}", error),
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
            Err(error) => println!("Error deleting keys: {:?}", error),
        }
    }

    pub fn environment(mut self) -> Self {
        let available_env = Environment::all();

        let env: &Environment = FuzzySelect::with_theme(&ColorfulTheme::default())
            .with_prompt("Environment:")
            .items(&available_env[..])
            .interact()
            .map(|index| available_env.get(index).unwrap())
            .unwrap();

        self.environment = Some(*env);
        self
    }

    pub fn account(mut self, trust: &mut TrustFacade) -> Self {
        let account = AccountSearchDialog::new().search(trust).build();
        match account {
            Ok(account) => self.account = Some(account),
            Err(error) => println!("Error searching account: {:?}", error),
        }
        self
    }
}
