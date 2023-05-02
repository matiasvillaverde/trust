use std::error::Error;

use crate::views::account_view::AccountView;
use dialoguer::{theme::ColorfulTheme, Input, Select};
use trust_core::Trust;
use trust_model::Account;
use trust_model::TransactionCategory;

pub struct TransactionDialogBuilder {
    amount: Decimal,
    currency: Currency,
    account: Option<Account>,
    category: Option<TransactionCategory>,
    result: Option<Result<Transaction, Box<dyn Error>>>,
}

impl TransactionDialogBuilder {
    pub fn new(category: TransactionCategory) -> Self {
        TransactionDialogBuilder {
            amount: Decimal::new(0, 0),
            currency: Currency::USD,
            account: None,
            category: category,
            result: None,
        }
    }

    pub fn build(mut self, trust: &mut Trust) -> TransactionDialogBuilder {
        //self.result = Some(trust.create_ account(&self.name, &self.description));
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

    pub fn amount(mut self) -> Self {

        // TODO: Show available amounts.
        self.name = Input::with_theme(&ColorfulTheme::default())
            .with_prompt("How do you want to {?}?", self.category.to_string())
            .interact_text()
            .unwrap();
        self
    }

    pub fn currency(mut self) -> Self {
        self.description = Input::with_theme(&ColorfulTheme::default())
            .with_prompt("In which currency your would like to {?} ?", self.category.to_string())
            .interact_text()
            .unwrap();
        self
    }

    pub fn account(mut self) -> Self {
        self.description = Input::with_theme(&ColorfulTheme::default())
            .with_prompt("Which account do you want to use for { }?", self.category.to_string())
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

    pub fn display(self) {
        match self
            .result
            .expect("No result found, did you forget to call search?")
        {
            Ok(account) => AccountView::display_account(account),
            Err(error) => println!("Error searching account: {:?}", error),
        }
    }

    pub fn search(mut self, trust: &mut Trust) -> Self {
        let accounts = trust.search_all_accounts();
        match accounts {
            Ok(accounts) => {
                let account = Select::with_theme(&ColorfulTheme::default())
                    .with_prompt("Which account do you want to see?")
                    .default(0)
                    .items(&accounts[..])
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
