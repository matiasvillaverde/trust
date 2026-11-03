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
            Err(error) => println!("Error closing trade: {error:?}"),
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
    use crate::dialogs::io::{scripted_push_select, scripted_reset};
    use crate::test_support::broker::{DialogBroker, SyncBehavior};
    use alpaca_broker::AlpacaBroker;
    use chrono::Utc;
    use core::TrustFacade;
    use db_sqlite::SqliteDatabase;
    use model::{
        Account, BrokerLog, Currency, DistributionResult, DraftTrade, Status, Trade, TradeBalance,
        TradeCategory, TradingVehicleCategory, TransactionCategory,
    };
    use rust_decimal_macros::dec;
    use std::io::{Error as IoError, ErrorKind};
    use uuid::Uuid;

    fn test_trust() -> TrustFacade {
        let path = std::env::temp_dir().join(format!("trust-test-{}.db", Uuid::new_v4()));
        let db = SqliteDatabase::new(path.to_str().expect("valid temp db path"));
        TrustFacade::new(Box::new(db), Box::<AlpacaBroker>::default())
    }

    fn test_trust_with_close_broker() -> TrustFacade {
        TrustFacade::new(
            Box::new(SqliteDatabase::new_in_memory()),
            Box::new(DialogBroker {
                sync: SyncBehavior::Filled,
                allow_close: true,
                ..DialogBroker::default()
            }),
        )
    }

    fn seed_filled_trade(trust: &mut TrustFacade, account_name: &str) -> (Account, Trade) {
        let account = trust
            .create_account(
                account_name,
                "desc",
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
    fn display_identifies_the_failed_action_and_its_cause() {
        crate::test_support::capture::assert_stdout(
            module_path!(),
            "display_identifies_the_failed_action_and_its_cause",
            || {
                CloseDialogBuilder {
                    account: None,
                    trade: None,
                    auto_distribute: false,
                    result: Some(Err("synthetic failure".into())),
                }
                .display();
            },
            |output| {
                assert!(output.contains("Error closing trade:"), "{output}");
                assert!(output.contains("synthetic failure"), "{output}");
                assert!(!output.contains("executed:"), "{output}");
            },
        );
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
    fn search_with_io_stores_trade_read_errors() {
        let mut trust = TrustFacade::new(
            Box::new(crate::test_support::ReadFailureFactory::trades()),
            Box::<AlpacaBroker>::default(),
        );
        let mut io = StubDialogIo {
            select_result: Ok(None),
        };

        let builder = CloseDialogBuilder {
            account: Some(Account {
                id: Uuid::new_v4(),
                ..Account::default()
            }),
            trade: None,
            auto_distribute: false,
            result: None,
        }
        .search_with_io(&mut trust, &mut io);

        let err = builder
            .result
            .expect("trade read error should set result")
            .expect_err("trade read should fail");
        assert!(err.to_string().contains("trade read failed"));
    }

    #[test]
    fn wrapper_methods_handle_default_console_paths() {
        let mut trust = test_trust();
        let account = trust
            .create_account(
                "close-wrapper",
                "test",
                model::Environment::Paper,
                dec!(20),
                dec!(10),
            )
            .expect("account");

        let missing_account = CloseDialogBuilder::new().account(&mut trust);
        assert!(missing_account.account.is_none());

        scripted_reset();
        scripted_push_select(Ok(Some(0)));
        let selected_account = CloseDialogBuilder::new().account(&mut trust);
        assert_eq!(
            selected_account.account.as_ref().map(|a| a.id),
            Some(account.id)
        );

        scripted_push_select(Ok(None));
        let empty_search = selected_account.search(&mut trust);
        assert!(empty_search.result.is_some());

        scripted_reset();
    }

    #[test]
    fn search_selects_filled_trade_and_manual_build_succeeds() {
        let mut trust = test_trust_with_close_broker();
        let (account, filled) = seed_filled_trade(&mut trust, "close-manual-success");
        let mut io = StubDialogIo {
            select_result: Ok(Some(0)),
        };

        let selected = CloseDialogBuilder {
            account: Some(account),
            trade: None,
            auto_distribute: false,
            result: None,
        }
        .search_with_io(&mut trust, &mut io);
        assert_eq!(
            selected.trade.as_ref().expect("selected trade").id,
            filled.id
        );

        let closed = selected.build(&mut trust);
        let (_balance, log, distribution) = closed
            .result
            .expect("close result should be set")
            .expect("manual close should succeed");
        assert_eq!(log.trade_id, filled.id);
        assert!(distribution.is_none());
    }

    #[test]
    fn close_requests_preserve_capital_and_defer_distribution_until_reconciliation() {
        for auto_distribute in [false, true] {
            let mut trust = test_trust_with_close_broker();
            let (account, filled) = seed_filled_trade(&mut trust, "pending-close");
            let balance = trust.search_balance(account.id, &Currency::USD).unwrap();
            let receipts = trust.get_account_transactions(account.id).unwrap();
            let closed = CloseDialogBuilder {
                account: Some(account.clone()),
                trade: Some(filled.clone()),
                auto_distribute,
                result: None,
            }
            .build(&mut trust);
            let (pending, log, distribution) = closed.result.unwrap().unwrap();
            assert_eq!(pending, filled.balance);
            assert_eq!(log.trade_id, filled.id);
            assert!(distribution.is_none());
            let persisted = trust.read_trade(filled.id).unwrap();
            assert_eq!(persisted.status, Status::Filled);
            assert_eq!(persisted.target.category, model::OrderCategory::Market);
            assert_eq!(persisted.safety_stop, filled.safety_stop);
            assert_eq!(
                trust.search_balance(account.id, &Currency::USD).unwrap(),
                balance
            );
            assert_eq!(
                trust.get_account_transactions(account.id).unwrap(),
                receipts
            );
            let replay = CloseDialogBuilder {
                account: Some(account.clone()),
                trade: Some(filled),
                auto_distribute,
                result: None,
            }
            .build(&mut trust);
            assert!(replay.result.unwrap().is_err());
            assert_eq!(trust.read_trade(persisted.id).unwrap(), persisted);
            assert_eq!(
                trust.get_account_transactions(account.id).unwrap(),
                receipts
            );
        }
    }

    #[test]
    fn close_receipt_reports_confirmed_distribution_record_count() {
        crate::test_support::capture::assert_stdout(
            module_path!(),
            "close_receipt_reports_confirmed_distribution_record_count",
            || {
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
                            insurance_amount: None,
                            distribution_date: Utc::now().naive_utc(),
                            transactions_created: vec![Uuid::new_v4()],
                        }),
                    ))),
                }
                .display();
            },
            |output| {
                assert!(output.contains("Trade close executed:"), "{output}");
                assert!(output.contains("Paper"), "{output}");
                assert!(
                    output.contains("Auto distribution executed: 1 transfer records"),
                    "{output}"
                );
            },
        );
    }

    #[test]
    fn close_without_distribution_does_not_claim_transfers() {
        crate::test_support::capture::assert_stdout(
            module_path!(),
            "close_without_distribution_does_not_claim_transfers",
            || {
                CloseDialogBuilder {
                    account: Some(Account {
                        name: "paper".to_string(),
                        ..Account::default()
                    }),
                    trade: Some(Trade {
                        status: Status::Filled,
                        ..Trade::default()
                    }),
                    auto_distribute: false,
                    result: Some(Ok((TradeBalance::default(), BrokerLog::default(), None))),
                }
                .display();
            },
            |output| {
                assert!(output.contains("Trade close executed:"), "{output}");
                assert!(output.contains("Paper"), "{output}");
                assert!(!output.contains("Auto distribution executed"), "{output}");
            },
        );
    }
}
