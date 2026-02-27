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
use crate::views::{LogView, TradeBalanceView, TradeView};
use core::TrustFacade;
use model::{Account, BrokerLog, Status, Trade};
use std::error::Error;
use std::io::ErrorKind;

type TradeDialogApproverBuilderResult = Option<Result<(Trade, BrokerLog), Box<dyn Error>>>;

pub struct SubmitDialogBuilder {
    account: Option<Account>,
    trade: Option<Trade>,
    result: TradeDialogApproverBuilderResult,
}

impl SubmitDialogBuilder {
    pub fn new() -> Self {
        SubmitDialogBuilder {
            account: None,
            trade: None,
            result: None,
        }
    }

    pub fn build(mut self, trust: &mut TrustFacade) -> SubmitDialogBuilder {
        let trade = match dialog_helpers::require(
            self.trade.clone(),
            ErrorKind::InvalidInput,
            "No trade selected for submission",
        ) {
            Ok(trade) => trade,
            Err(error) => {
                self.result = Some(Err(error));
                return self;
            }
        };

        self.result = Some(trust.submit_trade(&trade));
        self
    }

    pub fn display(self) {
        match self
            .result
            .expect("No result found, did you forget to call search?")
        {
            Ok((trade, log)) => {
                println!("Trade submitted:");
                let account_name = self
                    .account
                    .as_ref()
                    .map_or("<unknown account>", |account| account.name.as_str());
                TradeView::display(&trade, account_name);

                TradeBalanceView::display(&trade.balance);

                println!("Stop:");
                crate::views::OrderView::display(trade.safety_stop);

                println!("Entry:");
                crate::views::OrderView::display(trade.entry);

                println!("Target:");
                crate::views::OrderView::display(trade.target);

                LogView::display(&log);
            }
            Err(error) => println!("Error submitting trade: {error:?}"),
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

        let trades = trust.search_trades(account.id, Status::Funded);
        match trades {
            Ok(trades) => match dialog_helpers::select_from_list(
                io,
                "Trade:",
                &trades,
                "No funded trade found for this account",
                "Trade selection was canceled",
            ) {
                Ok(trade) => self.trade = Some(trade),
                Err(error) => self.result = Some(Err(error)),
            },
            Err(error) => self.result = Some(Err(error)),
        }

        self
    }
}

#[cfg(test)]
mod tests {
    use super::SubmitDialogBuilder;
    use alpaca_broker::AlpacaBroker;
    use core::TrustFacade;
    use db_sqlite::SqliteDatabase;
    use model::{
        Account, BrokerLog, Currency, DraftTrade, Trade, TradeCategory, TradingVehicleCategory,
        TransactionCategory,
    };
    use rust_decimal_macros::dec;
    use std::io::{Error as IoError, ErrorKind};
    use uuid::Uuid;

    fn test_trust() -> TrustFacade {
        let path = std::env::temp_dir().join(format!("trust-test-{}.db", Uuid::new_v4()));
        let db = SqliteDatabase::new(path.to_str().expect("valid temp db path"));
        TrustFacade::new(Box::new(db), Box::<AlpacaBroker>::default())
    }

    fn seed_funded_trade(trust: &mut TrustFacade) -> Account {
        let account = trust
            .create_account(
                "submit-seed",
                "test",
                model::Environment::Paper,
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
        let _ = trust.fund_trade(&trade).expect("fund");
        account
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

    #[test]
    fn new_starts_with_empty_state() {
        let builder = SubmitDialogBuilder::new();
        assert!(builder.account.is_none());
        assert!(builder.trade.is_none());
        assert!(builder.result.is_none());
    }

    #[test]
    fn build_returns_error_when_trade_is_missing() {
        let mut trust = test_trust();
        let builder = SubmitDialogBuilder::new().build(&mut trust);
        let err = builder
            .result
            .expect("result should be set")
            .expect_err("missing trade should fail");
        assert!(err.to_string().contains("No trade selected for submission"));
    }

    #[test]
    fn display_handles_error_result() {
        let builder = SubmitDialogBuilder {
            account: None,
            trade: None,
            result: Some(Err("synthetic failure".into())),
        };
        builder.display();
    }

    #[test]
    fn search_with_io_requires_account_and_handles_empty_list() {
        let mut trust = test_trust();
        let mut io = StubDialogIo {
            select_result: Ok(Some(0)),
        };
        let missing = SubmitDialogBuilder::new().search_with_io(&mut trust, &mut io);
        let missing_err = missing
            .result
            .expect("result should be set")
            .expect_err("missing account should fail");
        assert!(missing_err.to_string().contains("No account selected"));

        let account = trust
            .create_account(
                "submit-empty",
                "test",
                model::Environment::Paper,
                rust_decimal_macros::dec!(20),
                rust_decimal_macros::dec!(10),
            )
            .expect("account");
        let mut io = StubDialogIo {
            select_result: Ok(None),
        };
        let empty = SubmitDialogBuilder {
            account: Some(account),
            trade: None,
            result: None,
        }
        .search_with_io(&mut trust, &mut io);
        let empty_err = empty
            .result
            .expect("result should be set")
            .expect_err("empty list should fail");
        assert!(empty_err
            .to_string()
            .contains("No funded trade found for this account"));
    }

    #[test]
    fn search_with_io_handles_io_error() {
        let mut trust = test_trust();
        let account = seed_funded_trade(&mut trust);
        let mut io = StubDialogIo {
            select_result: Err(IoError::new(ErrorKind::TimedOut, "io failed")),
        };
        let builder = SubmitDialogBuilder {
            account: Some(account),
            trade: None,
            result: None,
        }
        .search_with_io(&mut trust, &mut io);
        let err = builder
            .result
            .expect("result should be set")
            .expect_err("io should fail");
        assert!(err.to_string().contains("Trade selection was canceled"));
    }

    #[test]
    fn display_handles_success_result() {
        let builder = SubmitDialogBuilder {
            account: Some(Account {
                name: "paper".to_string(),
                ..Account::default()
            }),
            trade: Some(Trade::default()),
            result: Some(Ok((Trade::default(), BrokerLog::default()))),
        };
        builder.display();
    }
}
