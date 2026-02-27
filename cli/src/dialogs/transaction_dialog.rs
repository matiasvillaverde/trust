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

use crate::dialogs::account_dialog::AccountSearchDialog;
use crate::dialogs::io::{ConsoleDialogIo, DialogIo};
use crate::views::{AccountBalanceView, TransactionView};
use core::TrustFacade;
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
                println!("Transaction created in account:  {name}");
                TransactionView::display(&transaction, &name);
                println!("Now the account {name} balance is:");
                AccountBalanceView::display(balance, &name);
            }
            Err(error) => println!("Error creating account: {error:?}"),
        }
    }

    pub fn amount(mut self, trust: &mut TrustFacade) -> Self {
        let mut io = ConsoleDialogIo::default();
        self = self.amount_with_io(trust, &mut io);
        self
    }

    pub fn amount_with_io(mut self, trust: &mut TrustFacade, io: &mut dyn DialogIo) -> Self {
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
                Err(error) => println!("Error searching account: {error:?}"),
            }
        }

        let input = io.input_text(&message, false);
        match input {
            Ok(raw) => match raw.parse::<Decimal>() {
                Ok(value) => self.amount = Some(value),
                Err(_) => println!("Please enter a valid number."),
            },
            Err(error) => println!("Error reading amount: {error}"),
        }
        self
    }

    pub fn currency(mut self, trust: &mut TrustFacade) -> Self {
        let mut io = ConsoleDialogIo::default();
        self = self.currency_with_io(trust, &mut io);
        self
    }

    pub fn currency_with_io(mut self, trust: &mut TrustFacade, io: &mut dyn DialogIo) -> Self {
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
                Err(error) => println!("Error searching account: {error:?}"),
            }
        } else {
            currencies = Currency::all();
        }

        let message = format!("How currency do you want to {}?", self.category);
        let labels: Vec<String> = currencies.iter().map(ToString::to_string).collect();
        match io.select_index(&message, &labels, 0) {
            Ok(Some(index)) => {
                self.currency = currencies.get(index).copied();
            }
            Ok(None) => {}
            Err(error) => println!("Error selecting currency: {error}"),
        }
        self
    }

    pub fn account(mut self, trust: &mut TrustFacade) -> Self {
        let account = AccountSearchDialog::new().search(trust).build();
        match account {
            Ok(account) => self.account = Some(account),
            Err(error) => println!("Error searching account: {error:?}"),
        }
        self
    }
}

#[cfg(test)]
mod tests {
    use super::TransactionDialogBuilder;
    use crate::dialogs::io::DialogIo;
    use crate::dialogs::io::{scripted_push_input, scripted_push_select, scripted_reset};
    use alpaca_broker::AlpacaBroker;
    use core::TrustFacade;
    use db_sqlite::SqliteDatabase;
    use model::AccountBalance;
    use model::{Account, Currency, Environment, Transaction, TransactionCategory};
    use rust_decimal_macros::dec;
    use std::collections::VecDeque;
    use std::io::{Error as IoError, ErrorKind};
    use uuid::Uuid;

    struct ScriptedIo {
        selects: VecDeque<Result<Option<usize>, IoError>>,
        inputs: VecDeque<Result<String, IoError>>,
    }

    impl ScriptedIo {
        fn with_selects(selects: Vec<Result<Option<usize>, IoError>>) -> Self {
            Self {
                selects: selects.into(),
                inputs: VecDeque::new(),
            }
        }

        fn with_inputs(inputs: Vec<Result<String, IoError>>) -> Self {
            Self {
                selects: VecDeque::new(),
                inputs: inputs.into(),
            }
        }
    }

    impl DialogIo for ScriptedIo {
        fn select_index(
            &mut self,
            _prompt: &str,
            _labels: &[String],
            _default: usize,
        ) -> Result<Option<usize>, IoError> {
            self.selects.pop_front().unwrap_or(Ok(None))
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
    fn new_starts_with_expected_values() {
        let builder = TransactionDialogBuilder::new(TransactionCategory::Deposit);
        assert!(builder.amount.is_none());
        assert!(builder.currency.is_none());
        assert!(builder.account.is_none());
        assert_eq!(builder.category, TransactionCategory::Deposit);
        assert!(builder.result.is_none());
    }

    #[test]
    fn display_handles_error_result() {
        TransactionDialogBuilder {
            amount: None,
            currency: None,
            account: None,
            category: TransactionCategory::Deposit,
            result: Some(Err("synthetic failure".into())),
        }
        .display();
    }

    #[test]
    #[should_panic(expected = "No account found, did you forget to call account?")]
    fn build_panics_when_required_inputs_are_missing() {
        let mut trust = test_trust();
        let _ = TransactionDialogBuilder::new(TransactionCategory::Deposit).build(&mut trust);
    }

    #[test]
    #[should_panic(expected = "No amount found, did you forget to call amount?")]
    fn build_panics_when_amount_is_missing() {
        let mut trust = test_trust();
        let _ = TransactionDialogBuilder {
            amount: None,
            currency: Some(Currency::USD),
            account: Some(Account::default()),
            category: TransactionCategory::Deposit,
            result: None,
        }
        .build(&mut trust);
    }

    #[test]
    #[should_panic(expected = "No currency found, did you forget to call currency?")]
    fn build_panics_when_currency_is_missing() {
        let mut trust = test_trust();
        let _ = TransactionDialogBuilder {
            amount: Some(dec!(10)),
            currency: None,
            account: Some(Account::default()),
            category: TransactionCategory::Deposit,
            result: None,
        }
        .build(&mut trust);
    }

    #[test]
    fn display_handles_success_result() {
        TransactionDialogBuilder {
            amount: Some(dec!(10)),
            currency: Some(Currency::USD),
            account: Some(Account {
                name: "paper".to_string(),
                ..Account::default()
            }),
            category: TransactionCategory::Deposit,
            result: Some(Ok((
                Transaction::new(
                    Uuid::new_v4(),
                    TransactionCategory::Deposit,
                    &Currency::USD,
                    dec!(10),
                ),
                model::AccountBalance::default(),
            ))),
        }
        .display();
    }

    #[test]
    fn build_creates_transaction_successfully() {
        let mut trust = test_trust();
        let account = trust
            .create_account("tx-build", "desc", Environment::Paper, dec!(20), dec!(10))
            .expect("account should be created");

        let built = TransactionDialogBuilder {
            amount: Some(dec!(25)),
            currency: Some(Currency::USD),
            account: Some(account.clone()),
            category: TransactionCategory::Deposit,
            result: None,
        }
        .build(&mut trust);

        let (tx, balance) = built
            .result
            .as_ref()
            .expect("result should be set")
            .as_ref()
            .expect("transaction creation should succeed");
        assert_eq!(tx.category, TransactionCategory::Deposit);
        assert_eq!(tx.amount, dec!(25));
        assert_eq!(tx.currency, Currency::USD);
        assert_eq!(balance.currency, Currency::USD);
        assert_eq!(balance.total_balance, dec!(25));
        assert_eq!(balance.total_available, dec!(25));
        assert_eq!(balance.total_in_trade, dec!(0));
        assert_eq!(balance.taxed, dec!(0));
        assert_eq!(balance.total_earnings, dec!(0));

        built.display();
    }

    #[test]
    fn amount_with_io_sets_amount_and_handles_invalid_or_io_error() {
        let mut trust = test_trust();
        let account = Account {
            id: Uuid::new_v4(),
            name: "paper".to_string(),
            ..Account::default()
        };

        let mut valid_io = ScriptedIo::with_inputs(vec![Ok("123.45".to_string())]);
        let valid = TransactionDialogBuilder {
            amount: None,
            currency: Some(Currency::USD),
            account: Some(account.clone()),
            category: TransactionCategory::Deposit,
            result: None,
        }
        .amount_with_io(&mut trust, &mut valid_io);
        assert_eq!(valid.amount, Some(dec!(123.45)));

        let mut invalid_io = ScriptedIo::with_inputs(vec![Ok("abc".to_string())]);
        let invalid = TransactionDialogBuilder {
            amount: Some(dec!(10)),
            currency: Some(Currency::USD),
            account: Some(account.clone()),
            category: TransactionCategory::Deposit,
            result: None,
        }
        .amount_with_io(&mut trust, &mut invalid_io);
        assert_eq!(invalid.amount, Some(dec!(10)));

        let mut err_io =
            ScriptedIo::with_inputs(vec![Err(IoError::new(ErrorKind::BrokenPipe, "io failed"))]);
        let err = TransactionDialogBuilder {
            amount: Some(dec!(7)),
            currency: Some(Currency::USD),
            account: Some(account),
            category: TransactionCategory::Deposit,
            result: None,
        }
        .amount_with_io(&mut trust, &mut err_io);
        assert_eq!(err.amount, Some(dec!(7)));
    }

    #[test]
    fn currency_with_io_sets_selected_currency_and_handles_cancel() {
        let mut trust = test_trust();
        let account = trust
            .create_account(
                "tx-currency",
                "desc",
                Environment::Paper,
                dec!(20),
                dec!(10),
            )
            .expect("account should be created");
        let _ = trust
            .create_transaction(
                &account,
                &TransactionCategory::Deposit,
                dec!(100),
                &Currency::USD,
            )
            .expect("deposit should succeed");

        let mut select_first = ScriptedIo::with_selects(vec![Ok(Some(0))]);
        let selected = TransactionDialogBuilder {
            amount: Some(dec!(10)),
            currency: None,
            account: Some(account.clone()),
            category: TransactionCategory::Withdrawal,
            result: None,
        }
        .currency_with_io(&mut trust, &mut select_first);
        assert_eq!(selected.currency, Some(Currency::USD));

        let mut cancel = ScriptedIo::with_selects(vec![Ok(None)]);
        let canceled = TransactionDialogBuilder {
            amount: Some(dec!(10)),
            currency: None,
            account: Some(account),
            category: TransactionCategory::Withdrawal,
            result: None,
        }
        .currency_with_io(&mut trust, &mut cancel);
        assert!(canceled.currency.is_none());
    }

    #[test]
    fn display_success_covers_non_default_balance_values() {
        TransactionDialogBuilder {
            amount: Some(dec!(10)),
            currency: Some(Currency::USD),
            account: Some(Account {
                name: "paper".to_string(),
                ..Account::default()
            }),
            category: TransactionCategory::Deposit,
            result: Some(Ok((
                Transaction::new(
                    Uuid::new_v4(),
                    TransactionCategory::Deposit,
                    &Currency::USD,
                    dec!(10),
                ),
                AccountBalance {
                    total_balance: dec!(110),
                    total_available: dec!(90),
                    total_in_trade: dec!(20),
                    taxed: dec!(5),
                    total_earnings: dec!(15),
                    currency: Currency::USD,
                    ..AccountBalance::default()
                },
            ))),
        }
        .display();
    }

    #[test]
    fn wrapper_methods_use_default_console_io_in_tests() {
        let mut trust = test_trust();
        let account = trust
            .create_account("tx-wrapper", "desc", Environment::Paper, dec!(20), dec!(10))
            .expect("account");
        trust
            .create_transaction(
                &account,
                &TransactionCategory::Deposit,
                dec!(1000),
                &Currency::USD,
            )
            .expect("deposit");

        scripted_reset();
        scripted_push_select(Ok(Some(0)));
        scripted_push_select(Ok(Some(0)));
        scripted_push_input(Ok("120".to_string()));

        let builder = TransactionDialogBuilder::new(TransactionCategory::Deposit)
            .account(&mut trust)
            .currency(&mut trust)
            .amount(&mut trust);
        assert_eq!(
            builder.account.as_ref().expect("selected account").id,
            account.id
        );
        assert_eq!(builder.currency, Some(Currency::USD));
        assert_eq!(builder.amount, Some(dec!(120)));
        scripted_reset();
    }
}
