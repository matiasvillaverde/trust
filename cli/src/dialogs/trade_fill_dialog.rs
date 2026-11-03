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
use crate::views::{TradeBalanceView, TradeView, TransactionView};
use core::TrustFacade;
use model::{Account, Status, Trade, Transaction};
use rust_decimal::Decimal;
use std::error::Error;

type EntryDialogBuilderResult = Option<Result<(Trade, Transaction), Box<dyn Error>>>;

pub struct FillTradeDialogBuilder {
    account: Option<Account>,
    trade: Option<Trade>,
    fee: Option<Decimal>,
    result: EntryDialogBuilderResult,
}

impl FillTradeDialogBuilder {
    pub fn new() -> Self {
        FillTradeDialogBuilder {
            account: None,
            trade: None,
            fee: None,
            result: None,
        }
    }

    pub fn build(mut self, trust: &mut TrustFacade) -> FillTradeDialogBuilder {
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
            trust.fill_trade(trade, fee)
        })());
        self
    }

    pub fn display(self) {
        match self
            .result
            .expect("No result found, did you forget to call search?")
        {
            Ok((trade, tx)) => {
                let name = self.account.unwrap().name;
                println!("Trade entry executed:");
                TradeView::display(&trade, name.as_str());
                TradeBalanceView::display(&trade.balance);
                TransactionView::display(&tx, name.as_str());
            }
            Err(error) => println!("Error filling trade: {error:?}"),
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

    pub fn fee(mut self) -> Self {
        let mut io = ConsoleDialogIo::default();
        self = self.fee_with_io(&mut io);
        self
    }

    pub fn fee_with_io(mut self, io: &mut dyn DialogIo) -> Self {
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
            let trades = trust.search_trades(account.id, Status::Submitted)?;
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
}

#[cfg(test)]
mod tests {
    use super::FillTradeDialogBuilder;
    use crate::dialogs::io::DialogIo;
    use crate::dialogs::io::{scripted_push_input, scripted_push_select, scripted_reset};
    use crate::test_support::broker::DialogBroker;
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
            Box::new(DialogBroker::default()),
        )
    }

    fn seed_submitted_trade(trust: &mut TrustFacade, account_name: &str) -> (Account, Trade) {
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
        (account, submitted)
    }

    #[test]
    fn display_identifies_the_failed_action_and_its_cause() {
        crate::test_support::capture::assert_stdout(
            module_path!(),
            "display_identifies_the_failed_action_and_its_cause",
            || {
                FillTradeDialogBuilder {
                    account: None,
                    trade: None,
                    fee: None,
                    result: Some(Err("synthetic failure".into())),
                }
                .display();
            },
            |output| {
                assert!(output.contains("Error filling trade:"), "{output}");
                assert!(output.contains("synthetic failure"), "{output}");
                assert!(!output.contains("executed:"), "{output}");
            },
        );
    }

    #[test]
    fn build_rejects_without_trade() {
        let mut trust = test_trust();
        let before = trust.search_all_accounts().unwrap();
        let rejected = FillTradeDialogBuilder::new().build(&mut trust);

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
    fn build_rejects_without_fee() {
        let mut trust = test_trust();
        let before = trust.search_all_accounts().unwrap();
        let rejected = FillTradeDialogBuilder {
            account: Some(Account::default()),
            trade: Some(Trade::default()),
            fee: None,
            result: None,
        }
        .build(&mut trust);

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
        let ok = FillTradeDialogBuilder::new().fee_with_io(&mut io);
        assert_eq!(ok.fee, Some(dec!(1.25)));

        io.inputs.push_back(Ok("-1".to_string()));
        let negative = FillTradeDialogBuilder {
            fee: Some(dec!(2)),
            ..FillTradeDialogBuilder::new()
        }
        .fee_with_io(&mut io);
        assert_eq!(negative.fee, Some(dec!(2)));

        io.inputs.push_back(Ok("abc".to_string()));
        let invalid = FillTradeDialogBuilder {
            fee: Some(dec!(3)),
            ..FillTradeDialogBuilder::new()
        }
        .fee_with_io(&mut io);
        assert_eq!(invalid.fee, Some(dec!(3)));

        io.inputs
            .push_back(Err(IoError::new(ErrorKind::BrokenPipe, "io failed")));
        let errored = FillTradeDialogBuilder {
            fee: Some(dec!(4)),
            ..FillTradeDialogBuilder::new()
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
        let rejected = FillTradeDialogBuilder {
            account: Some(account.clone()),
            ..FillTradeDialogBuilder::new()
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

        let builder = FillTradeDialogBuilder {
            account: Some(Account {
                id: Uuid::new_v4(),
                ..Account::default()
            }),
            ..FillTradeDialogBuilder::new()
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

        let builder = FillTradeDialogBuilder::new().account(&mut trust);

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
        let rejected = FillTradeDialogBuilder {
            account: Some(account.clone()),
            ..FillTradeDialogBuilder::new()
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
    fn search_with_io_selects_submitted_trade_and_handles_non_selection_paths() {
        let mut trust = test_trust_with_submit_broker();
        let (account, submitted) = seed_submitted_trade(&mut trust, "fill-search-select");

        let mut selected_io = ScriptedIo::new();
        selected_io.selects.push_back(Ok(Some(0)));
        let selected = FillTradeDialogBuilder {
            account: Some(account.clone()),
            ..FillTradeDialogBuilder::new()
        }
        .search_with_io(&mut trust, &mut selected_io);
        assert_eq!(selected.trade.expect("selected trade").id, submitted.id);

        let mut canceled_io = ScriptedIo::new();
        canceled_io.selects.push_back(Ok(None));
        let canceled = FillTradeDialogBuilder {
            account: Some(account.clone()),
            ..FillTradeDialogBuilder::new()
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
        let errored = FillTradeDialogBuilder {
            account: Some(account),
            ..FillTradeDialogBuilder::new()
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
    fn manual_fill_records_exact_quantity_and_fee_once() {
        crate::test_support::capture::assert_stdout(
            module_path!(),
            "manual_fill_records_exact_quantity_and_fee_once",
            || {
                let mut trust = test_trust_with_submit_broker();
                let (account, mut submitted) = seed_submitted_trade(&mut trust, "fractional-fill");
                submitted.entry.average_filled_price = Some(dec!(100));
                let filled = FillTradeDialogBuilder {
                    account: Some(account.clone()),
                    trade: Some(submitted.clone()),
                    fee: Some(dec!(1.25)),
                    result: None,
                }
                .build(&mut trust);
                let (trade, receipt) = filled.result.as_ref().unwrap().as_ref().unwrap();
                assert_eq!(trade.status, Status::Filled);
                assert_eq!(receipt.amount, dec!(1000));
                assert_eq!(trade.balance.capital_in_market, dec!(1000));
                // Open-trade cash flow includes the $1,000 purchase and $1.25 fee.
                assert_eq!(trade.balance.total_performance, dec!(-1001.25));
                assert_eq!(&trust.read_trade(trade.id).unwrap(), trade);
                assert_eq!(
                    trust
                        .search_balance(account.id, &Currency::USD)
                        .unwrap()
                        .total_available,
                    dec!(8998.75)
                );
                let receipts = trust.get_account_transactions(account.id).unwrap();
                assert_eq!(
                    receipts
                        .iter()
                        .filter(|tx| tx.category == TransactionCategory::FeeOpen(trade.id))
                        .map(|tx| tx.amount)
                        .sum::<rust_decimal::Decimal>(),
                    dec!(1.25)
                );
                assert!(FillTradeDialogBuilder {
                    account: Some(account.clone()),
                    trade: Some(submitted),
                    fee: Some(dec!(1.25)),
                    result: None
                }
                .build(&mut trust)
                .result
                .unwrap()
                .is_err());
                assert_eq!(
                    trust.get_account_transactions(account.id).unwrap(),
                    receipts
                );

                filled.display();
            },
            |output| {
                assert!(output.contains("Trade entry executed:"), "{output}");
                assert!(output.contains("Fractional-fill"), "{output}");
                assert!(output.contains("filled"), "{output}");
                assert!(output.contains("1000"), "{output}");
                assert!(output.contains("-1001.25"), "{output}");
                assert!(output.contains("open_trade"), "{output}");
            },
        );
    }

    #[test]
    fn wrapper_account_and_fee_use_default_console_io_in_tests() {
        let mut trust = test_trust();
        let account = trust
            .create_account("fill-wrap", "desc", Environment::Paper, dec!(20), dec!(10))
            .expect("account");

        scripted_reset();
        scripted_push_select(Ok(Some(0)));
        scripted_push_input(Ok("1.5".to_string()));
        let builder = FillTradeDialogBuilder::new().account(&mut trust).fee();
        assert_eq!(
            builder.account.as_ref().expect("selected account").id,
            account.id
        );
        assert_eq!(builder.fee, Some(dec!(1.5)));
        scripted_reset();
    }
}
