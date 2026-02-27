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

use crate::dialogs::io::{ConsoleDialogIo, DialogIo};
use crate::dialogs::AccountSearchDialog;
use crate::views::{AccountBalanceView, TradeBalanceView, TradeView, TransactionView};
use core::TrustFacade;
use model::{Account, AccountBalance, Status, Trade, TradeBalance, Transaction};
use rust_decimal::Decimal;
use std::error::Error;

type ExitDialogBuilderResult =
    Option<Result<(Transaction, Transaction, TradeBalance, AccountBalance), Box<dyn Error>>>;

pub struct ExitDialogBuilder {
    account: Option<Account>,
    trade: Option<Trade>,
    fee: Option<Decimal>,
    result: ExitDialogBuilderResult,
}

impl ExitDialogBuilder {
    pub fn new() -> Self {
        ExitDialogBuilder {
            account: None,
            trade: None,
            fee: None,
            result: None,
        }
    }

    pub fn build_stop(mut self, trust: &mut TrustFacade) -> ExitDialogBuilder {
        let trade: Trade = self
            .trade
            .clone()
            .expect("No trade found, did you forget to select one?");
        let fee = self
            .fee
            .expect("No fee found, did you forget to specify a fee?");
        self.result = Some(trust.stop_trade(&trade, fee));
        self
    }

    pub fn build_target(mut self, trust: &mut TrustFacade) -> ExitDialogBuilder {
        let trade: Trade = self
            .trade
            .clone()
            .expect("No trade found, did you forget to select one?");
        let fee = self
            .fee
            .expect("No fee found, did you forget to specify a fee?");
        self.result = Some(trust.target_acquired(&trade, fee));
        self
    }

    pub fn display(self) {
        match self
            .result
            .expect("No result found, did you forget to call search?")
        {
            Ok((tx_exit, tx_payment, trade_balance, account_balance)) => {
                let account_name = self.account.clone().unwrap().name;

                println!("Trade exit executed:");
                TradeView::display(&self.trade.unwrap(), account_name.as_str());

                println!("With transaction of exit:");
                TransactionView::display(&tx_exit, account_name.as_str());

                println!("With transaction of payment back to the account:");
                TransactionView::display(&tx_payment, account_name.as_str());

                TradeBalanceView::display(&trade_balance);

                AccountBalanceView::display(account_balance, account_name.as_str());
            }
            Err(error) => println!("Error approving trade: {error:?}"),
        }
    }

    pub fn account(mut self, trust: &mut TrustFacade) -> Self {
        let account = AccountSearchDialog::new().search(trust).build();
        match account {
            Ok(account) => self.account = Some(account),
            Err(error) => println!("Error searching account: {error:?}"),
        }
        self
    }

    pub fn search(mut self, trust: &mut TrustFacade) -> Self {
        let mut io = ConsoleDialogIo::default();
        self = self.search_with_io(trust, &mut io);
        self
    }

    fn search_with_io(mut self, trust: &mut TrustFacade, io: &mut dyn DialogIo) -> Self {
        let trades = trust.search_trades(self.account.clone().unwrap().id, Status::Filled);
        match trades {
            Ok(trades) => {
                if trades.is_empty() {
                    panic!("No trade found, did you forget to create one?")
                }
                let labels: Vec<String> = trades.iter().map(ToString::to_string).collect();
                match io.select_index("Trade:", &labels, 0) {
                    Ok(Some(index)) => self.trade = trades.get(index).cloned(),
                    Ok(None) => {}
                    Err(error) => self.result = Some(Err(Box::new(error))),
                }
            }
            Err(error) => self.result = Some(Err(error)),
        }

        self
    }

    pub fn fee(mut self) -> Self {
        let mut io = ConsoleDialogIo::default();
        self = self.fee_with_io(&mut io);
        self
    }

    fn fee_with_io(mut self, io: &mut dyn DialogIo) -> Self {
        match io.input_text("Fee", false) {
            Ok(raw) => match raw.parse::<Decimal>() {
                Ok(parsed) if parsed.is_sign_negative() => {
                    println!("Please enter a positive fee");
                }
                Ok(parsed) => self.fee = Some(parsed),
                Err(_) => println!("Please enter a valid number for the fee"),
            },
            Err(error) => println!("Error reading fee: {error}"),
        }
        self
    }
}

#[cfg(test)]
mod tests {
    use super::ExitDialogBuilder;
    use crate::dialogs::io::DialogIo;
    use crate::dialogs::io::{scripted_push_input, scripted_push_select, scripted_reset};
    use alpaca_broker::AlpacaBroker;
    use core::TrustFacade;
    use db_sqlite::SqliteDatabase;
    use model::{Account, Currency, Environment, Trade, Transaction, TransactionCategory};
    use rust_decimal_macros::dec;
    use std::collections::VecDeque;
    use std::io::{Error as IoError, ErrorKind};
    use uuid::Uuid;

    struct ScriptedIo {
        selects: VecDeque<Result<Option<usize>, IoError>>,
        inputs: VecDeque<Result<String, IoError>>,
    }

    impl ScriptedIo {
        fn new() -> Self {
            Self {
                selects: VecDeque::new(),
                inputs: VecDeque::new(),
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
    fn new_starts_with_empty_state() {
        let builder = ExitDialogBuilder::new();
        assert!(builder.account.is_none());
        assert!(builder.trade.is_none());
        assert!(builder.fee.is_none());
        assert!(builder.result.is_none());
    }

    #[test]
    fn display_handles_error_result() {
        ExitDialogBuilder {
            account: None,
            trade: None,
            fee: None,
            result: Some(Err("synthetic failure".into())),
        }
        .display();
    }

    #[test]
    #[should_panic(expected = "No trade found, did you forget to select one?")]
    fn build_stop_panics_without_trade() {
        let mut trust = test_trust();
        let _ = ExitDialogBuilder::new().build_stop(&mut trust);
    }

    #[test]
    #[should_panic(expected = "No trade found, did you forget to select one?")]
    fn build_target_panics_without_trade() {
        let mut trust = test_trust();
        let _ = ExitDialogBuilder::new().build_target(&mut trust);
    }

    #[test]
    #[should_panic(expected = "No fee found, did you forget to specify a fee?")]
    fn build_stop_panics_without_fee() {
        let mut trust = test_trust();
        let _ = ExitDialogBuilder {
            account: Some(Account::default()),
            trade: Some(Trade::default()),
            fee: None,
            result: None,
        }
        .build_stop(&mut trust);
    }

    #[test]
    fn display_handles_success_result() {
        ExitDialogBuilder {
            account: Some(Account {
                name: "paper".to_string(),
                ..Account::default()
            }),
            trade: Some(Trade::default()),
            fee: Some(dec!(0)),
            result: Some(Ok((
                Transaction::new(
                    Uuid::new_v4(),
                    TransactionCategory::Deposit,
                    &Currency::USD,
                    dec!(10),
                ),
                Transaction::new(
                    Uuid::new_v4(),
                    TransactionCategory::Withdrawal,
                    &Currency::USD,
                    dec!(5),
                ),
                model::TradeBalance::default(),
                model::AccountBalance::default(),
            ))),
        }
        .display();
    }

    #[test]
    fn fee_with_io_handles_success_invalid_negative_and_error() {
        let mut io = ScriptedIo::new();
        io.inputs.push_back(Ok("1.25".to_string()));
        let ok = ExitDialogBuilder::new().fee_with_io(&mut io);
        assert_eq!(ok.fee, Some(dec!(1.25)));

        io.inputs.push_back(Ok("-1".to_string()));
        let negative = ExitDialogBuilder {
            fee: Some(dec!(2)),
            ..ExitDialogBuilder::new()
        }
        .fee_with_io(&mut io);
        assert_eq!(negative.fee, Some(dec!(2)));

        io.inputs.push_back(Ok("abc".to_string()));
        let invalid = ExitDialogBuilder {
            fee: Some(dec!(3)),
            ..ExitDialogBuilder::new()
        }
        .fee_with_io(&mut io);
        assert_eq!(invalid.fee, Some(dec!(3)));

        io.inputs
            .push_back(Err(IoError::new(ErrorKind::BrokenPipe, "io failed")));
        let errored = ExitDialogBuilder {
            fee: Some(dec!(4)),
            ..ExitDialogBuilder::new()
        }
        .fee_with_io(&mut io);
        assert_eq!(errored.fee, Some(dec!(4)));
    }

    #[test]
    fn search_with_io_panics_when_no_filled_trades_exist() {
        let mut trust = test_trust();
        let account = trust
            .create_account(
                "exit-search",
                "desc",
                Environment::Paper,
                dec!(20),
                dec!(10),
            )
            .expect("account");
        let mut io = ScriptedIo::new();
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _ = ExitDialogBuilder {
                account: Some(account),
                ..ExitDialogBuilder::new()
            }
            .search_with_io(&mut trust, &mut io);
        }));
        assert!(result.is_err());
    }

    #[test]
    fn wrapper_account_and_fee_use_default_console_io_in_tests() {
        let mut trust = test_trust();
        let account = trust
            .create_account("exit-wrap", "desc", Environment::Paper, dec!(20), dec!(10))
            .expect("account");

        scripted_reset();
        scripted_push_select(Ok(Some(0)));
        scripted_push_input(Ok("2.0".to_string()));
        let builder = ExitDialogBuilder::new().account(&mut trust).fee();
        assert_eq!(
            builder.account.as_ref().expect("selected account").id,
            account.id
        );
        assert_eq!(builder.fee, Some(dec!(2.0)));
        scripted_reset();
    }
}
