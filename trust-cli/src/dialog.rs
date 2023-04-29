use std::error::Error;

use crate::view::AccountView;
use dialoguer::{theme::ColorfulTheme, Input};
use trust_core::Trust;
use trust_model::Account;

pub struct AccountDialogBuilder {
    name: String,
    description: String,
    result: Option<Result<Account, Box<dyn Error>>>,
}

impl AccountDialogBuilder {
    pub fn new() -> Self {
        AccountDialogBuilder {
            name: "".to_string(),
            description: "".to_string(),
            result: None,
        }
    }

    pub fn build(mut self, trust: &mut Trust) -> AccountDialogBuilder {
        self.result = Some(trust.create_account(&self.name, &self.description));
        self
    }

    pub fn display(self) {
        match self
            .result
            .expect("No result found, did you forget to call build?")
        {
            Ok(account) => AccountView::display_account(account),
            Err(error) => println!("Error creating account: {:?}", error),
        }
    }

    pub fn name(mut self) -> Self {
        self.name = Input::with_theme(&ColorfulTheme::default())
            .with_prompt("How is the name of your trading account? For example: 'Robin Hood'")
            .interact_text()
            .unwrap();
        self
    }

    pub fn description(mut self) -> Self {
        self.description = Input::with_theme(&ColorfulTheme::default())
            .with_prompt("How would you describe your account?")
            .interact_text()
            .unwrap();
        self
    }
}
