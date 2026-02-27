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
use core::services::{AdvisoryAlertLevel, TradeProposal};
use core::TrustFacade;
use model::{Account, AccountBalance, Status, Trade, TradeBalance, Transaction};
use rust_decimal::Decimal;
use std::error::Error;
use std::io::ErrorKind;

type TradeDialogApproverBuilderResult =
    Option<Result<(Trade, Transaction, AccountBalance, TradeBalance), Box<dyn Error>>>;

pub struct FundingDialogBuilder {
    account: Option<Account>,
    trade: Option<Trade>,
    result: TradeDialogApproverBuilderResult,
}

impl FundingDialogBuilder {
    pub fn new() -> Self {
        FundingDialogBuilder {
            account: None,
            trade: None,
            result: None,
        }
    }

    pub fn build(self, trust: &mut TrustFacade) -> FundingDialogBuilder {
        let mut io = ConsoleDialogIo::default();
        self.build_with_io(trust, &mut io)
    }

    fn build_with_io(
        mut self,
        trust: &mut TrustFacade,
        io: &mut dyn DialogIo,
    ) -> FundingDialogBuilder {
        let trade = match dialog_helpers::require(
            self.trade.clone(),
            ErrorKind::InvalidInput,
            "No trade selected for funding",
        ) {
            Ok(trade) => trade,
            Err(error) => {
                self.result = Some(Err(error));
                return self;
            }
        };
        let advisory = trust.advisory_check_trade(TradeProposal {
            account_id: trade.account_id,
            symbol: trade.trading_vehicle.symbol.clone(),
            sector: trade.sector.clone(),
            asset_class: trade.asset_class.clone(),
            entry_price: trade.entry.unit_price,
            quantity: Decimal::from(trade.entry.quantity),
        });

        match advisory {
            Ok(result) => {
                match result.level {
                    AdvisoryAlertLevel::Block => {
                        self.result = Some(Err("Trade blocked by advisory limits".into()));
                        return self;
                    }
                    AdvisoryAlertLevel::Warning | AdvisoryAlertLevel::Caution => {
                        println!("Advisory {:?}:", result.level);
                        for warning in result.warnings {
                            println!("  - {warning}");
                        }
                        let proceed = io
                            .confirm("Proceed with funding anyway?", false)
                            .unwrap_or(false);
                        if !proceed {
                            self.result =
                                Some(Err("Funding canceled by user after advisory".into()));
                            return self;
                        }
                    }
                    AdvisoryAlertLevel::Ok => {}
                }
                self.result = Some(trust.fund_trade(&trade));
            }
            Err(error) => {
                self.result = Some(Err(error));
            }
        }
        self
    }

    pub fn display(self) {
        match self
            .result
            .expect("No result found, did you forget to call search?")
        {
            Ok((trade, tx, account_balance, trade_balance)) => {
                let account = self.account.clone().unwrap().name;

                println!("Trade approved:");
                TradeView::display(&trade, &self.account.unwrap().name);

                TradeBalanceView::display(&trade_balance);

                println!("Transaction moving funds to trade:");
                TransactionView::display(&tx, account.as_str());

                println!("Account balance after funding trade:");
                AccountBalanceView::display(account_balance, account.as_str());
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

        let trades = trust.search_trades(account.id, Status::New);
        match trades {
            Ok(trades) => match dialog_helpers::select_from_list(
                io,
                "Trade:",
                &trades,
                "No new trade found for this account",
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
    use super::FundingDialogBuilder;
    use crate::dialogs::io::{scripted_push_select, scripted_reset};
    use alpaca_broker::AlpacaBroker;
    use core::TrustFacade;
    use db_sqlite::SqliteDatabase;
    use model::{
        Account, Currency, DraftTrade, Environment, Trade, TradeCategory, TradingVehicleCategory,
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

    struct StubDialogIo {
        select_result: Result<Option<usize>, IoError>,
        confirm_result: Result<bool, IoError>,
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
            std::mem::replace(&mut self.confirm_result, Ok(false))
        }
    }

    fn seed_new_trade(trust: &mut TrustFacade, name: &str) -> (Account, Trade) {
        let account = trust
            .create_account(name, "test", Environment::Paper, dec!(20), dec!(10))
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
        (account, trade)
    }

    #[test]
    fn new_starts_with_empty_state() {
        let builder = FundingDialogBuilder::new();
        assert!(builder.account.is_none());
        assert!(builder.trade.is_none());
        assert!(builder.result.is_none());
    }

    #[test]
    fn display_handles_error_result() {
        FundingDialogBuilder {
            account: None,
            trade: None,
            result: Some(Err("synthetic failure".into())),
        }
        .display();
    }

    #[test]
    fn build_returns_error_when_trade_is_missing() {
        let mut trust = test_trust();
        let builder = FundingDialogBuilder::new().build(&mut trust);
        let err = builder
            .result
            .expect("result should be set")
            .expect_err("missing trade should fail");
        assert!(err.to_string().contains("No trade selected for funding"));
    }

    #[test]
    fn build_with_unknown_account_surfaces_advisory_error() {
        let mut trust = test_trust();
        let mut io = StubDialogIo {
            select_result: Ok(Some(0)),
            confirm_result: Ok(true),
        };
        let builder = FundingDialogBuilder {
            account: Some(Account::default()),
            trade: Some(Trade::default()),
            result: None,
        }
        .build_with_io(&mut trust, &mut io);
        assert!(builder.result.is_some());
        let _ = builder
            .result
            .expect("result")
            .expect_err("unknown account should fail");
    }

    #[test]
    fn search_with_io_requires_account_and_handles_empty_cancel_error() {
        let mut trust = test_trust();
        let mut io = StubDialogIo {
            select_result: Ok(Some(0)),
            confirm_result: Ok(true),
        };
        let missing = FundingDialogBuilder::new().search_with_io(&mut trust, &mut io);
        assert!(missing
            .result
            .expect("result")
            .expect_err("missing account should fail")
            .to_string()
            .contains("No account selected"));

        let account = trust
            .create_account("fund-empty", "test", Environment::Paper, dec!(20), dec!(10))
            .expect("account");
        let mut io = StubDialogIo {
            select_result: Ok(None),
            confirm_result: Ok(true),
        };
        let empty = FundingDialogBuilder {
            account: Some(account.clone()),
            trade: None,
            result: None,
        }
        .search_with_io(&mut trust, &mut io);
        assert!(empty
            .result
            .expect("result")
            .expect_err("empty should fail")
            .to_string()
            .contains("No new trade found for this account"));

        let (seeded_account, _seeded_trade) = seed_new_trade(&mut trust, "fund-seeded");
        let mut io = StubDialogIo {
            select_result: Err(IoError::new(ErrorKind::Interrupted, "dialog down")),
            confirm_result: Ok(true),
        };
        let cancel = FundingDialogBuilder {
            account: Some(seeded_account),
            trade: None,
            result: None,
        }
        .search_with_io(&mut trust, &mut io);
        assert!(cancel
            .result
            .expect("result")
            .expect_err("io should fail")
            .to_string()
            .contains("Trade selection was canceled"));
    }

    #[test]
    fn search_with_io_selects_trade() {
        let mut trust = test_trust();
        let (account, trade) = seed_new_trade(&mut trust, "fund-select");
        let mut io = StubDialogIo {
            select_result: Ok(Some(0)),
            confirm_result: Ok(true),
        };

        let builder = FundingDialogBuilder {
            account: Some(account),
            trade: None,
            result: None,
        }
        .search_with_io(&mut trust, &mut io);
        assert!(builder.result.is_none());
        assert_eq!(builder.trade.expect("selected").id, trade.id);
    }

    #[test]
    fn wrapper_account_and_search_use_default_console_io_in_tests() {
        let mut trust = test_trust();
        let (account, trade) = seed_new_trade(&mut trust, "fund-wrap");

        scripted_reset();
        scripted_push_select(Ok(Some(0))); // account()
        scripted_push_select(Ok(Some(0))); // search()
        let builder = FundingDialogBuilder::new()
            .account(&mut trust)
            .search(&mut trust);
        assert_eq!(
            builder.account.as_ref().expect("selected account").id,
            account.id
        );
        assert_eq!(builder.trade.as_ref().expect("selected trade").id, trade.id);
        scripted_reset();
    }
}
