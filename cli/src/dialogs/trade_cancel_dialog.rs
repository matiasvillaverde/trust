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

use crate::dialogs::{dialog_helpers, AccountSearchDialog, ConsoleDialogIo, DialogIo};
use crate::views::{AccountBalanceView, TradeBalanceView, TradeView, TransactionView};
use core::TrustFacade;
use model::{Account, AccountBalance, Status, Trade, TradeBalance, Transaction};
use std::error::Error;
use std::io::ErrorKind;

type CancelDialogBuilderResult =
    Option<Result<(TradeBalance, AccountBalance, Transaction), Box<dyn Error>>>;

pub struct CancelDialogBuilder {
    account: Option<Account>,
    trade: Option<Trade>,
    result: CancelDialogBuilderResult,
}

impl CancelDialogBuilder {
    pub fn new() -> Self {
        CancelDialogBuilder {
            account: None,
            trade: None,
            result: None,
        }
    }

    pub fn build(mut self, trust: &mut TrustFacade) -> CancelDialogBuilder {
        let trade: Trade = match dialog_helpers::require(
            self.trade.clone(),
            ErrorKind::InvalidInput,
            "No trade selected for cancellation",
        ) {
            Ok(trade) => trade,
            Err(error) => {
                self.result = Some(Err(error));
                return self;
            }
        };

        match trade.status {
            Status::Funded => {
                self.result = Some(trust.cancel_funded_trade(&trade));
            }
            Status::Submitted => {
                self.result = Some(trust.cancel_submitted_trade(&trade));
            }
            _ => {
                self.result = Some(Err(Box::new(std::io::Error::new(
                    ErrorKind::InvalidInput,
                    "Trade is not in a cancellable state",
                ))));
            }
        }

        self
    }

    pub fn display(self) {
        match self
            .result
            .expect("No result found, did you forget to call search?")
        {
            Ok((trade_balance, account_o, tx)) => {
                let account_name = self.account.clone().unwrap().name;

                println!("Trade cancel executed:");
                TradeView::display(&self.trade.unwrap(), account_name.as_str());
                TradeBalanceView::display(&trade_balance);
                AccountBalanceView::display(account_o, account_name.as_str());
                TransactionView::display(&tx, account_name.as_str());
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

    pub fn search(self, trust: &mut TrustFacade) -> Self {
        let mut io = ConsoleDialogIo::default();
        self.search_with_io(trust, &mut io)
    }

    fn search_with_io(mut self, trust: &mut TrustFacade, io: &mut dyn DialogIo) -> Self {
        let account = match dialog_helpers::require(
            self.account.clone(),
            ErrorKind::InvalidInput,
            "No account selected",
        ) {
            Ok(account) => account,
            Err(error) => {
                self.result = Some(Err(error));
                return self;
            }
        };

        let funded_trades = trust
            .search_trades(account.id, Status::Funded)
            .unwrap_or_default();
        let submitted_trades = trust
            .search_trades(account.id, Status::Submitted)
            .unwrap_or_default();

        let trades = funded_trades
            .into_iter()
            .chain(submitted_trades)
            .collect::<Vec<Trade>>();

        match dialog_helpers::select_from_list(
            io,
            "Trade:",
            &trades,
            "No funded or submitted trades found for this account",
            "Trade selection was canceled",
        ) {
            Ok(trade) => self.trade = Some(trade),
            Err(error) => self.result = Some(Err(error)),
        }

        self
    }
}

#[cfg(test)]
mod tests {
    use super::CancelDialogBuilder;
    use alpaca_broker::AlpacaBroker;
    use core::TrustFacade;
    use db_sqlite::SqliteDatabase;
    use model::{
        Account, Currency, DraftTrade, Environment, Status, Trade, TradeCategory,
        TradingVehicleCategory, TransactionCategory,
    };
    use rust_decimal_macros::dec;
    use std::io::{Error as IoError, ErrorKind};
    use uuid::Uuid;

    fn test_trust() -> TrustFacade {
        let path = std::env::temp_dir().join(format!("trust-test-{}.db", Uuid::new_v4()));
        let db = SqliteDatabase::new(path.to_str().expect("valid temp db path"));
        TrustFacade::new(Box::new(db), Box::<AlpacaBroker>::default())
    }

    struct StubDialogIo {
        select_result: Result<Option<usize>, IoError>,
    }

    impl crate::dialogs::DialogIo for StubDialogIo {
        fn select_index(
            &mut self,
            _prompt: &str,
            _labels: &[String],
            _default: usize,
        ) -> Result<Option<usize>, IoError> {
            std::mem::replace(&mut self.select_result, Ok(None))
        }

        fn confirm(&mut self, _prompt: &str, _default: bool) -> Result<bool, IoError> {
            Ok(false)
        }
    }

    fn seed_funded_trade(trust: &mut TrustFacade) -> (Account, Trade) {
        let account = trust
            .create_account(
                "cancel-seed",
                "test",
                Environment::Paper,
                dec!(20),
                dec!(10),
            )
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
            quantity: 10,
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
        let (funded, _, _, _) = trust.fund_trade(&trade).expect("funded");
        (account, funded)
    }

    #[test]
    fn new_starts_with_empty_state() {
        let builder = CancelDialogBuilder::new();
        assert!(builder.account.is_none());
        assert!(builder.trade.is_none());
        assert!(builder.result.is_none());
    }

    #[test]
    fn build_returns_error_when_trade_is_missing() {
        let mut trust = test_trust();
        let builder = CancelDialogBuilder::new().build(&mut trust);
        let err = builder
            .result
            .expect("result should be set")
            .expect_err("missing trade should fail");
        assert!(err
            .to_string()
            .contains("No trade selected for cancellation"));
    }

    #[test]
    fn build_returns_error_when_trade_is_not_cancellable() {
        let mut trust = test_trust();
        let builder = CancelDialogBuilder {
            account: None,
            trade: Some(Trade {
                status: Status::Filled,
                ..Trade::default()
            }),
            result: None,
        }
        .build(&mut trust);

        let err = builder
            .result
            .expect("result should be set")
            .expect_err("filled trade is not cancellable");
        assert!(err
            .to_string()
            .contains("Trade is not in a cancellable state"));
    }

    #[test]
    fn search_with_io_requires_account_and_handles_empty_list() {
        let mut trust = test_trust();
        let mut io = StubDialogIo {
            select_result: Ok(Some(0)),
        };

        let missing_account = CancelDialogBuilder::new().search_with_io(&mut trust, &mut io);
        let missing_error = missing_account
            .result
            .expect("result should be set")
            .expect_err("missing account should fail");
        assert!(missing_error.to_string().contains("No account selected"));

        let account = trust
            .create_account(
                "empty-cancel",
                "test",
                Environment::Paper,
                dec!(20),
                dec!(10),
            )
            .expect("account");
        let mut io = StubDialogIo {
            select_result: Ok(None),
        };
        let empty = CancelDialogBuilder {
            account: Some(account),
            trade: None,
            result: None,
        }
        .search_with_io(&mut trust, &mut io);
        let empty_error = empty
            .result
            .expect("result should be set")
            .expect_err("empty trade list should fail");
        assert!(empty_error
            .to_string()
            .contains("No funded or submitted trades found for this account"));
    }

    #[test]
    fn search_with_io_selects_trade() {
        let mut trust = test_trust();
        let (account, trade) = seed_funded_trade(&mut trust);
        let mut io = StubDialogIo {
            select_result: Ok(Some(0)),
        };

        let builder = CancelDialogBuilder {
            account: Some(account),
            trade: None,
            result: None,
        }
        .search_with_io(&mut trust, &mut io);

        assert!(builder.result.is_none());
        assert_eq!(builder.trade.expect("trade selected").id, trade.id);
    }

    #[test]
    fn search_with_io_handles_io_error() {
        let mut trust = test_trust();
        let (account, _trade) = seed_funded_trade(&mut trust);
        let mut io = StubDialogIo {
            select_result: Err(IoError::new(ErrorKind::Interrupted, "broken dialog")),
        };

        let builder = CancelDialogBuilder {
            account: Some(account),
            trade: None,
            result: None,
        }
        .search_with_io(&mut trust, &mut io);
        let err = builder
            .result
            .expect("result should be set")
            .expect_err("io error should fail");
        assert!(err.to_string().contains("Trade selection was canceled"));
    }

    #[test]
    fn display_handles_error_result() {
        CancelDialogBuilder {
            account: None,
            trade: None,
            result: Some(Err("synthetic failure".into())),
        }
        .display();
    }

    #[test]
    fn build_and_display_successfully_for_funded_trade() {
        let mut trust = test_trust();
        let (account, funded_trade) = seed_funded_trade(&mut trust);

        let builder = CancelDialogBuilder {
            account: Some(account),
            trade: Some(funded_trade),
            result: None,
        }
        .build(&mut trust);

        let (trade_balance, account_balance, tx) = builder
            .result
            .as_ref()
            .expect("result should be set")
            .as_ref()
            .expect("funded trade should cancel");
        assert!(trade_balance.funding >= dec!(0));
        assert!(account_balance.total_balance >= dec!(0));
        assert!(!tx.id.is_nil());

        builder.display();
    }
}
