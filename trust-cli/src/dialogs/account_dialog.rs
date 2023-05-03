use std::error::Error;

use crate::views::account_view::{AccountOverviewView, AccountView};
use dialoguer::{theme::ColorfulTheme, FuzzySelect, Input};
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

    pub fn display(self, trust: &mut Trust) {
        match self
            .result
            .expect("No result found, did you forget to call search?")
        {
            Ok(account) => {
                let overviews = trust
                    .search_all_overviews(account.id)
                    .expect("Error searching account overviews");
                let name = account.name.clone();
                AccountView::display_account(account);
                AccountOverviewView::display_overviews(overviews, &name)
            }
            Err(error) => println!("Error searching account: {:?}", error),
        }
    }

    pub fn search(mut self, trust: &mut Trust) -> Self {
        let accounts = trust.search_all_accounts();
        match accounts {
            Ok(accounts) => {
                let account = FuzzySelect::with_theme(&ColorfulTheme::default())
                    .with_prompt("Which account do you want to use?")
                    .items(&accounts[..])
                    .default(0)
                    .interact_opt()
                    .unwrap()
                    .map(|index| accounts.get(index).unwrap())
                    .unwrap();

                self.result = Some(Ok(account.to_owned()));
            }
            Err(error) => self.result = Some(Err(error)),
        }

        self
    }
}
