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
        self.result = Some((|| {
            let trade = crate::dialogs::dialog_helpers::require(
                self.trade.as_ref(),
                std::io::ErrorKind::InvalidInput,
                "Select a trade",
            )?;
            let fee = crate::dialogs::dialog_helpers::require(
                self.fee,
                std::io::ErrorKind::InvalidInput,
                "Enter a fee",
            )?;
            trust.stop_trade(trade, fee)
        })());
        self
    }

    pub fn build_target(mut self, trust: &mut TrustFacade) -> ExitDialogBuilder {
        self.result = Some((|| {
            let trade = crate::dialogs::dialog_helpers::require(
                self.trade.as_ref(),
                std::io::ErrorKind::InvalidInput,
                "Select a trade",
            )?;
            let fee = crate::dialogs::dialog_helpers::require(
                self.fee,
                std::io::ErrorKind::InvalidInput,
                "Enter a fee",
            )?;
            trust.target_acquired(trade, fee)
        })());
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
            Err(error) => println!("Error exiting trade: {error:?}"),
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
        let selected = (|| {
            let account = crate::dialogs::dialog_helpers::require(
                self.account.as_ref(),
                std::io::ErrorKind::InvalidInput,
                "Select an account",
            )?;
            let trades = trust.search_trades(account.id, Status::Filled)?;
            crate::dialogs::dialog_helpers::select_from_list(
                io,
                "Trade:",
                &trades,
                "No eligible trades found",
                "Trade selection cancelled",
            )
        })();
        match selected {
            Ok(trade) => self.trade = Some(trade),
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
    use crate::test_support::broker::{DialogBroker, SyncBehavior};
    use alpaca_broker::AlpacaBroker;
    use core::TrustFacade;
    use db_sqlite::SqliteDatabase;
    use model::{
        Account, Currency, DraftTrade, Environment, Status, Trade, TradeCategory,
        TradingVehicleCategory, TransactionCategory,
    };
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

        fn input_text(&mut self, _prompt: &str, _allow_empty: bool) -> Result<String, IoError> {
            self.inputs.pop_front().unwrap_or_else(|| Ok(String::new()))
        }
    }

    fn test_trust() -> TrustFacade {
        let path = std::env::temp_dir().join(format!("trust-test-{}.db", Uuid::new_v4()));
        let db = SqliteDatabase::new(path.to_str().expect("valid temp db path"));
        TrustFacade::new(Box::new(db), Box::<AlpacaBroker>::default())
    }

    fn test_trust_with_submit_broker() -> TrustFacade {
        TrustFacade::new(
            Box::new(SqliteDatabase::new_in_memory()),
            Box::new(DialogBroker {
                sync: SyncBehavior::Filled,
                ..DialogBroker::default()
            }),
        )
    }

    fn seed_filled_trade(trust: &mut TrustFacade, account_name: &str) -> (Account, Trade) {
        let account = trust
            .create_account(account_name, "desc", Environment::Paper, dec!(20), dec!(10))
            .expect("account");
        trust
            .create_transaction(
                &account,
                &TransactionCategory::Deposit,
                dec!(10_000),
                &Currency::USD,
            )
            .expect("deposit");
        let vehicle = trust
            .create_trading_vehicle("AAPL", None, &TradingVehicleCategory::Stock, "alpaca")
            .expect("vehicle");
        let draft = DraftTrade {
            account: account.clone(),
            trading_vehicle: vehicle,
            quantity: 10.into(),
            currency: Currency::USD,
            category: TradeCategory::Long,
            thesis: None,
            sector: None,
            asset_class: None,
            context: None,
        };
        let trade = trust
            .create_trade(draft, dec!(95), dec!(100), dec!(110))
            .expect("trade");
        let (funded, _, _, _) = trust.fund_trade(&trade).expect("fund trade");
        let (submitted, _) = trust.submit_trade(&funded).expect("submit trade");
        trust.sync_trade(&submitted, &account).expect("sync trade");
        let filled = trust
            .search_trades(account.id, Status::Filled)
            .expect("filled search")
            .pop()
            .expect("filled trade");
        (account, filled)
    }

    #[test]
    fn display_identifies_the_failed_action_and_its_cause() {
        crate::test_support::capture::assert_stdout(
            module_path!(),
            "display_identifies_the_failed_action_and_its_cause",
            || {
                ExitDialogBuilder {
                    account: None,
                    trade: None,
                    fee: None,
                    result: Some(Err("synthetic failure".into())),
                }
                .display();
            },
            |output| {
                assert!(output.contains("Error exiting trade:"), "{output}");
                assert!(output.contains("synthetic failure"), "{output}");
                assert!(!output.contains("executed:"), "{output}");
            },
        );
    }

    #[test]
    fn build_stop_rejects_without_trade() {
        let mut trust = test_trust();
        let before = trust.search_all_accounts().unwrap();
        let rejected = ExitDialogBuilder::new().build_stop(&mut trust);

        let error = rejected
            .result
            .unwrap()
            .expect_err("incomplete input must be rejected");
        assert_eq!(
            error
                .downcast_ref::<std::io::Error>()
                .map(std::io::Error::kind),
            Some(std::io::ErrorKind::InvalidInput)
        );
        assert_eq!(trust.search_all_accounts().unwrap(), before);
    }

    #[test]
    fn build_target_rejects_without_trade() {
        let mut trust = test_trust();
        let before = trust.search_all_accounts().unwrap();
        let rejected = ExitDialogBuilder::new().build_target(&mut trust);

        let error = rejected
            .result
            .unwrap()
            .expect_err("incomplete input must be rejected");
        assert_eq!(
            error
                .downcast_ref::<std::io::Error>()
                .map(std::io::Error::kind),
            Some(std::io::ErrorKind::InvalidInput)
        );
        assert_eq!(trust.search_all_accounts().unwrap(), before);
    }

    #[test]
    fn build_stop_rejects_without_fee() {
        let mut trust = test_trust();
        let before = trust.search_all_accounts().unwrap();
        let rejected = ExitDialogBuilder {
            account: Some(Account::default()),
            trade: Some(Trade::default()),
            fee: None,
            result: None,
        }
        .build_stop(&mut trust);

        let error = rejected
            .result
            .unwrap()
            .expect_err("incomplete input must be rejected");
        assert_eq!(
            error
                .downcast_ref::<std::io::Error>()
                .map(std::io::Error::kind),
            Some(std::io::ErrorKind::InvalidInput)
        );
        assert_eq!(trust.search_all_accounts().unwrap(), before);
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
    fn empty_selection_is_reported_without_financial_writes_search_with_io() {
        let mut trust = test_trust();
        let account = trust
            .create_account("empty-selection", "", Environment::Paper, dec!(0), dec!(0))
            .unwrap();
        let before = trust.get_account_transactions(account.id).unwrap();
        let mut io = ScriptedIo::new();
        let rejected = ExitDialogBuilder {
            account: Some(account.clone()),
            ..ExitDialogBuilder::new()
        }
        .search_with_io(&mut trust, &mut io);
        assert!(rejected.trade.is_none());
        let error = rejected.result.unwrap().unwrap_err();
        assert_eq!(
            error
                .downcast_ref::<std::io::Error>()
                .map(std::io::Error::kind),
            Some(std::io::ErrorKind::NotFound)
        );
        assert_eq!(trust.get_account_transactions(account.id).unwrap(), before);
    }

    #[test]
    fn search_with_io_stores_trade_read_errors() {
        let mut trust = TrustFacade::new(
            Box::new(crate::test_support::ReadFailureFactory::trades()),
            Box::<AlpacaBroker>::default(),
        );
        let mut io = ScriptedIo::new();

        let builder = ExitDialogBuilder {
            account: Some(Account {
                id: Uuid::new_v4(),
                ..Account::default()
            }),
            ..ExitDialogBuilder::new()
        }
        .search_with_io(&mut trust, &mut io);

        let err = builder
            .result
            .expect("trade read error should set result")
            .expect_err("trade read should fail");
        assert!(err.to_string().contains("trade read failed"));
    }

    #[test]
    fn account_wrapper_handles_search_error() {
        let mut trust = test_trust();
        scripted_reset();

        let builder = ExitDialogBuilder::new().account(&mut trust);

        assert!(builder.account.is_none());
        scripted_reset();
    }

    #[test]
    fn empty_selection_is_reported_without_financial_writes_search() {
        let mut trust = test_trust();
        let account = trust
            .create_account("empty-selection", "", Environment::Paper, dec!(0), dec!(0))
            .unwrap();
        let before = trust.get_account_transactions(account.id).unwrap();
        scripted_reset();
        let rejected = ExitDialogBuilder {
            account: Some(account.clone()),
            ..ExitDialogBuilder::new()
        }
        .search(&mut trust);
        assert!(rejected.trade.is_none());
        let error = rejected.result.unwrap().unwrap_err();
        assert_eq!(
            error
                .downcast_ref::<std::io::Error>()
                .map(std::io::Error::kind),
            Some(std::io::ErrorKind::NotFound)
        );
        assert_eq!(trust.get_account_transactions(account.id).unwrap(), before);
    }

    #[test]
    fn search_with_io_selects_filled_trade_and_handles_non_selection_paths() {
        let mut trust = test_trust_with_submit_broker();
        let (account, filled) = seed_filled_trade(&mut trust, "exit-search-select");

        let mut selected_io = ScriptedIo::new();
        selected_io.selects.push_back(Ok(Some(0)));
        let selected = ExitDialogBuilder {
            account: Some(account.clone()),
            ..ExitDialogBuilder::new()
        }
        .search_with_io(&mut trust, &mut selected_io);
        assert_eq!(selected.trade.expect("selected trade").id, filled.id);

        let mut canceled_io = ScriptedIo::new();
        canceled_io.selects.push_back(Ok(None));
        let canceled = ExitDialogBuilder {
            account: Some(account.clone()),
            ..ExitDialogBuilder::new()
        }
        .search_with_io(&mut trust, &mut canceled_io);
        assert!(canceled.trade.is_none());
        assert_eq!(
            canceled
                .result
                .unwrap()
                .unwrap_err()
                .downcast_ref::<IoError>()
                .map(IoError::kind),
            Some(ErrorKind::InvalidInput)
        );

        let mut error_io = ScriptedIo::new();
        error_io
            .selects
            .push_back(Err(IoError::new(ErrorKind::BrokenPipe, "select failed")));
        let errored = ExitDialogBuilder {
            account: Some(account),
            ..ExitDialogBuilder::new()
        }
        .search_with_io(&mut trust, &mut error_io);
        let err = errored
            .result
            .expect("selection error should set result")
            .expect_err("selection error should fail");
        assert!(err.to_string().contains("select failed"));

        let mut default_io = ScriptedIo::new();
        assert!(default_io
            .select_index("unused", &[], 0)
            .expect("default select")
            .is_none());
        assert_eq!(
            default_io
                .input_text("unused", false)
                .expect("default input"),
            ""
        );
    }

    #[test]
    fn manual_exits_account_for_adverse_prices_fees_and_reject_replays() {
        crate::test_support::capture::assert_stdout(
            module_path!(),
            "manual_exits_account_for_adverse_prices_fees_and_reject_replays",
            || {
                for stopped in [true, false] {
                    let mut trust = test_trust_with_submit_broker();
                    let (account, mut filled) = seed_filled_trade(&mut trust, "fractional-exit");
                    filled.safety_stop.average_filled_price = Some(dec!(94.5));
                    filled.target.average_filled_price = Some(dec!(110.25));
                    let builder = ExitDialogBuilder {
                        account: Some(account.clone()),
                        trade: Some(filled.clone()),
                        fee: Some(dec!(1.25)),
                        result: None,
                    };
                    let result = if stopped {
                        builder.build_stop(&mut trust)
                    } else {
                        builder.build_target(&mut trust)
                    };
                    let (exit, _, balance, _) = result.result.as_ref().unwrap().as_ref().unwrap();
                    assert_eq!(exit.amount, if stopped { dec!(945) } else { dec!(1102.5) });
                    assert_eq!(balance.capital_in_market, dec!(0));
                    assert_eq!(
                        balance.total_performance,
                        if stopped { dec!(-56.25) } else { dec!(101.25) }
                    );
                    result.display();
                    let persisted = trust.read_trade(filled.id).unwrap();
                    assert_eq!(
                        persisted.status,
                        if stopped {
                            Status::ClosedStopLoss
                        } else {
                            Status::ClosedTarget
                        }
                    );
                    let receipts = trust.get_account_transactions(account.id).unwrap();
                    let replay = ExitDialogBuilder {
                        account: Some(account.clone()),
                        trade: Some(filled),
                        fee: Some(dec!(1.25)),
                        result: None,
                    };
                    assert!(if stopped {
                        replay.build_stop(&mut trust)
                    } else {
                        replay.build_target(&mut trust)
                    }
                    .result
                    .unwrap()
                    .is_err());
                    assert_eq!(trust.read_trade(persisted.id).unwrap(), persisted);
                    assert_eq!(
                        trust.get_account_transactions(account.id).unwrap(),
                        receipts
                    );
                }
            },
            |output| {
                assert!(output.contains("Trade exit executed:"), "{output}");
                assert!(output.contains("Fractional-exit"), "{output}");
                assert!(output.contains("945"), "{output}");
                assert!(output.contains("1102.5"), "{output}");
                assert!(output.contains("-56.25"), "{output}");
                assert!(output.contains("101.25"), "{output}");
                assert!(output.contains("close_safety_stop"), "{output}");
                assert!(output.contains("close_target"), "{output}");
                assert!(output.contains("payment_from_trade"), "{output}");
            },
        );
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
