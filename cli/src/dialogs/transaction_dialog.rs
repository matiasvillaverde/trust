use crate::dialogs::account_dialog::AccountSearchDialog;
use crate::views::{AccountBalanceView, TransactionView};
use core::TrustFacade;
use dialoguer::{theme::ColorfulTheme, FuzzySelect, Input};
use model::Account;
use model::AccountBalance;
use model::Currency;
use model::Transaction;
use model::TransactionCategory;
use rust_decimal::Decimal;
use std::error::Error;

pub struct TransactionDialogBuilder {
    amount: Option<Decimal>,
    currency: Option<Currency>,
    account: Option<Account>,
    category: TransactionCategory,
    result: Option<Result<(Transaction, AccountBalance), Box<dyn Error>>>,
}

impl TransactionDialogBuilder {
    pub fn new(category: TransactionCategory) -> Self {
        TransactionDialogBuilder {
            amount: None,
            currency: None,
            account: None,
            category,
            result: None,
        }
    }

    pub fn build(mut self, trust: &mut TrustFacade) -> TransactionDialogBuilder {
        self.result = Some(
            trust.create_transaction(
                &self
                    .account
                    .clone()
                    .expect("No account found, did you forget to call account?"),
                &self.category,
                self.amount
                    .expect("No amount found, did you forget to call amount?"),
                &self
                    .currency
                    .expect("No currency found, did you forget to call currency?"),
            ),
        );
        self
    }

    pub fn display(self) {
        match self
            .result
            .expect("No result found, did you forget to call build?")
        {
            Ok((transaction, balance)) => {
                let name = self.account.unwrap().name;
                println!("Transaction created in account:  {}", name);
                TransactionView::display(&transaction, &name);
                println!("Now the account {} balance is:", name);
                AccountBalanceView::display(balance, &name);
            }
            Err(error) => println!("Error creating account: {:?}", error),
        }
    }

    pub fn amount(mut self, trust: &mut TrustFacade) -> Self {
        let message = format!("How much do you want to {}?", self.category);

        // Show available if withdrawal.
        if self.category == TransactionCategory::Withdrawal {
            let account_id = self
                .account
                .clone()
                .expect("No account found, did you forget to call account?")
                .id;
            let currency = self
                .currency
                .expect("No currency found, did you forget to call currency?");
            let balance = trust.search_balance(account_id, &currency);
            match balance {
                Ok(balance) => {
                    println!(
                        "Available for withdrawal: {} {}",
                        balance.total_available, balance.currency
                    );
                }
                Err(error) => println!("Error searching account: {:?}", error),
            }
        }

        let amount = Input::with_theme(&ColorfulTheme::default())
            .with_prompt(message)
            .validate_with({
                |input: &String| -> Result<(), &str> {
                    match input.parse::<Decimal>() {
                        Ok(_) => Ok(()),
                        Err(_) => Err("Please enter a valid number."),
                    }
                }
            })
            .interact_text()
            .unwrap()
            .parse::<Decimal>()
            .unwrap();

        self.amount = Some(amount);
        self
    }

    pub fn currency(mut self, trust: &mut TrustFacade) -> Self {
        let mut currencies = Vec::new();

        if self.category == TransactionCategory::Withdrawal {
            let account_id = self
                .account
                .clone()
                .expect("No account found, did you forget to call account?")
                .id;
            let balances = trust.search_all_balances(account_id);
            match balances {
                Ok(balances) => {
                    for balance in balances {
                        currencies.push(balance.currency);
                    }
                }
                Err(error) => println!("Error searching account: {:?}", error),
            }
        } else {
            currencies = Currency::all();
        }

        let message = format!("How currency do you want to {}?", self.category);

        let selected_currency = FuzzySelect::with_theme(&ColorfulTheme::default())
            .with_prompt(message)
            .items(&currencies[..])
            .interact()
            .map(|index| currencies.get(index).unwrap())
            .unwrap();

        self.currency = Some(*selected_currency);
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
