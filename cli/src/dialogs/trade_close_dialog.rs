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
use model::{Account, BrokerLog, DistributionResult, Status, Trade, TradeBalance};
use std::error::Error;
use std::io::ErrorKind;

type CancelDialogBuilderResult =
    Option<Result<(TradeBalance, BrokerLog, Option<DistributionResult>), Box<dyn Error>>>;

pub struct CloseDialogBuilder {
    account: Option<Account>,
    trade: Option<Trade>,
    auto_distribute: bool,
    result: CancelDialogBuilderResult,
}

impl CloseDialogBuilder {
    pub fn new() -> Self {
        CloseDialogBuilder {
            account: None,
            trade: None,
            auto_distribute: false,
            result: None,
        }
    }

    pub fn build(mut self, trust: &mut TrustFacade) -> CloseDialogBuilder {
        let trade = match dialog_helpers::require(
            self.trade.clone(),
            ErrorKind::InvalidInput,
            "No trade selected for close",
        ) {
            Ok(trade) => trade,
            Err(error) => {
                self.result = Some(Err(error));
                return self;
            }
        };

        self.result = if self.auto_distribute {
            Some(trust.close_trade_with_auto_distribution(&trade))
        } else {
            Some(
                trust
                    .close_trade(&trade)
                    .map(|(balance, log)| (balance, log, None)),
            )
        };
        self
    }

    pub fn display(self) {
        match self
            .result
            .expect("No result found, did you forget to call search?")
        {
            Ok((trade_balance, log, distribution_result)) => {
                let account_name = self.account.clone().unwrap().name;

                println!("Trade close executed:");
                TradeView::display(&self.trade.unwrap(), account_name.as_str());
                TradeBalanceView::display(&trade_balance);
                LogView::display(&log);
                if let Some(distribution) = distribution_result {
                    println!(
                        "Auto distribution executed: {} transfer records",
                        distribution.transactions_created.len()
                    );
                }
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

        let trades = trust.search_trades(account.id, Status::Filled);
        match trades {
            Ok(trades) => match dialog_helpers::select_from_list(
                io,
                "Trade:",
                &trades,
                "No filled trades found for this account",
                "Trade selection was canceled",
            ) {
                Ok(trade) => self.trade = Some(trade),
                Err(error) => self.result = Some(Err(error)),
            },
            Err(error) => self.result = Some(Err(error)),
        }

        self
    }

    pub fn auto_distribute(mut self, auto_distribute: bool) -> Self {
        self.auto_distribute = auto_distribute;
        self
    }
}

#[cfg(test)]
mod tests {
    use super::CloseDialogBuilder;
    use alpaca_broker::AlpacaBroker;
    use chrono::Utc;
    use core::TrustFacade;
    use db_sqlite::SqliteDatabase;
    use model::{Account, BrokerLog, DistributionResult, Status, Trade, TradeBalance};
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

    #[test]
    fn new_starts_with_empty_state() {
        let builder = CloseDialogBuilder::new();
        assert!(builder.account.is_none());
        assert!(builder.trade.is_none());
        assert!(!builder.auto_distribute);
        assert!(builder.result.is_none());
    }

    #[test]
    fn auto_distribute_sets_flag() {
        let builder = CloseDialogBuilder::new().auto_distribute(true);
        assert!(builder.auto_distribute);
    }

    #[test]
    fn build_returns_error_when_trade_is_missing() {
        let mut trust = test_trust();
        let builder = CloseDialogBuilder::new().build(&mut trust);
        let err = builder
            .result
            .expect("result should be set")
            .expect_err("missing trade should fail");
        assert!(err.to_string().contains("No trade selected for close"));
    }

    #[test]
    fn display_handles_error_result() {
        CloseDialogBuilder {
            account: None,
            trade: None,
            auto_distribute: false,
            result: Some(Err("synthetic failure".into())),
        }
        .display();
    }

    #[test]
    fn search_with_io_requires_account_and_handles_empty_list_and_io_error() {
        let mut trust = test_trust();
        let mut io = StubDialogIo {
            select_result: Ok(Some(0)),
        };
        let missing = CloseDialogBuilder::new().search_with_io(&mut trust, &mut io);
        let missing_err = missing
            .result
            .expect("result should be set")
            .expect_err("missing account should fail");
        assert!(missing_err.to_string().contains("No account selected"));

        let account = trust
            .create_account(
                "close-empty",
                "test",
                model::Environment::Paper,
                dec!(20),
                dec!(10),
            )
            .expect("account");
        let mut io = StubDialogIo {
            select_result: Ok(None),
        };
        let empty = CloseDialogBuilder {
            account: Some(account.clone()),
            trade: None,
            auto_distribute: false,
            result: None,
        }
        .search_with_io(&mut trust, &mut io);
        let empty_err = empty
            .result
            .expect("result should be set")
            .expect_err("empty list should fail");
        assert!(empty_err
            .to_string()
            .contains("No filled trades found for this account"));

        let mut io = StubDialogIo {
            select_result: Err(IoError::new(ErrorKind::BrokenPipe, "io failed")),
        };
        let io_error = CloseDialogBuilder {
            account: Some(account),
            trade: Some(Trade {
                status: Status::Filled,
                ..Trade::default()
            }),
            auto_distribute: false,
            result: None,
        }
        .search_with_io(&mut trust, &mut io);
        let err = io_error
            .result
            .expect("result should be set")
            .expect_err("io should fail");
        assert!(err
            .to_string()
            .contains("No filled trades found for this account"));
    }

    #[test]
    fn display_handles_success_result_with_distribution() {
        CloseDialogBuilder {
            account: Some(Account {
                name: "paper".to_string(),
                ..Account::default()
            }),
            trade: Some(Trade {
                status: Status::Filled,
                ..Trade::default()
            }),
            auto_distribute: true,
            result: Some(Ok((
                TradeBalance::default(),
                BrokerLog::default(),
                Some(DistributionResult {
                    source_account_id: Uuid::new_v4(),
                    original_amount: dec!(10),
                    earnings_amount: Some(dec!(3)),
                    tax_amount: Some(dec!(2)),
                    reinvestment_amount: Some(dec!(5)),
                    distribution_date: Utc::now().naive_utc(),
                    transactions_created: vec![Uuid::new_v4()],
                }),
            ))),
        }
        .display();
    }
}
