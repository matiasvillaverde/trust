use std::error::Error;

use crate::views::{AccountBalanceView, AccountView, RuleView};
use core::TrustFacade;
use dialoguer::{theme::ColorfulTheme, FuzzySelect, Input};
use model::{Account, Environment};
use rust_decimal::Decimal;

pub struct AccountDialogBuilder {
    name: String,
    description: String,
    environment: Option<Environment>,
    tax_percentage: Option<Decimal>,
    earnings_percentage: Option<Decimal>,
    result: Option<Result<Account, Box<dyn Error>>>,
}

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
        self.name = Input::with_theme(&ColorfulTheme::default())
            .with_prompt("Name: ")
            .interact_text()
            .unwrap();
        self
    }

    pub fn description(mut self) -> Self {
        self.description = Input::with_theme(&ColorfulTheme::default())
            .with_prompt("Description: ")
            .interact_text()
            .unwrap();
        self
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

    pub fn tax_percentage(mut self) -> Self {
        let taxes = Input::new()
            .with_prompt("Taxes percentage")
            .interact()
            .unwrap();

        self.tax_percentage = Some(taxes);
        self
    }

    pub fn earnings_percentage(mut self) -> Self {
        let percentage = Input::new()
            .with_prompt("Earning percentage")
            .interact()
            .unwrap();

        self.earnings_percentage = Some(percentage);
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
        let accounts = trust.search_all_accounts();
        match accounts {
            Ok(accounts) => {
                if accounts.is_empty() {
                    panic!("No accounts found, did you forget to create one?")
                }
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
