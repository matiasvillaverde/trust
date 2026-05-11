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
            quantity: trade.entry.quantity,
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
        Account, Broker, BrokerKind, BrokerLog, Currency, DraftTrade, Environment, Order, OrderIds,
        Status, Trade, TradeCategory, TradingVehicleCategory, TransactionCategory,
    };
    use rust_decimal::Decimal;
    use rust_decimal_macros::dec;
    use std::io::{Error as IoError, ErrorKind};
    use uuid::Uuid;

    fn test_trust() -> TrustFacade {
        let path = std::env::temp_dir().join(format!("trust-test-{}.db", Uuid::new_v4()));
        let db = SqliteDatabase::new(path.to_str().expect("valid temp db path"));
        TrustFacade::new(Box::new(db), Box::<AlpacaBroker>::default())
    }

    fn test_trust_with_broker(broker: Box<dyn Broker>) -> TrustFacade {
        let path = std::env::temp_dir().join(format!("trust-test-{}.db", Uuid::new_v4()));
        let db = SqliteDatabase::new(path.to_str().expect("valid temp db path"));
        TrustFacade::new(Box::new(db), broker)
    }

    struct SubmitOkBroker;

    impl Broker for SubmitOkBroker {
        fn kind(&self) -> BrokerKind {
            BrokerKind::Alpaca
        }

        fn submit_trade(
            &self,
            trade: &Trade,
            _account: &Account,
        ) -> Result<(BrokerLog, OrderIds), Box<dyn std::error::Error>> {
            Ok((
                BrokerLog {
                    trade_id: trade.id,
                    log: "submitted".to_string(),
                    ..BrokerLog::default()
                },
                OrderIds {
                    stop: "stop-id".to_string(),
                    entry: "entry-id".to_string(),
                    target: "target-id".to_string(),
                },
            ))
        }

        fn sync_trade(
            &self,
            trade: &Trade,
            _account: &Account,
        ) -> Result<(Status, Vec<Order>, BrokerLog), Box<dyn std::error::Error>> {
            Ok((
                Status::Submitted,
                vec![],
                BrokerLog {
                    trade_id: trade.id,
                    log: "synced".to_string(),
                    ..BrokerLog::default()
                },
            ))
        }

        fn close_trade(
            &self,
            trade: &Trade,
            _account: &Account,
        ) -> Result<(Order, BrokerLog), Box<dyn std::error::Error>> {
            Ok((
                Order::default(),
                BrokerLog {
                    trade_id: trade.id,
                    log: "closed".to_string(),
                    ..BrokerLog::default()
                },
            ))
        }

        fn cancel_trade(
            &self,
            _trade: &Trade,
            _account: &Account,
        ) -> Result<(), Box<dyn std::error::Error>> {
            Ok(())
        }

        fn modify_stop(
            &self,
            _trade: &Trade,
            _account: &Account,
            _new_stop_price: Decimal,
        ) -> Result<String, Box<dyn std::error::Error>> {
            Ok("stop-id".to_string())
        }

        fn modify_target(
            &self,
            _trade: &Trade,
            _account: &Account,
            _new_price: Decimal,
        ) -> Result<String, Box<dyn std::error::Error>> {
            Ok("target-id".to_string())
        }
    }

    #[test]
    fn submit_ok_broker_support_methods_return_deterministic_values() {
        let broker = SubmitOkBroker;
        let trade = Trade::default();
        let account = Account::default();

        assert_eq!(broker.kind(), BrokerKind::Alpaca);

        let (submit_log, order_ids) = broker
            .submit_trade(&trade, &account)
            .expect("submit should succeed");
        assert_eq!(submit_log.trade_id, trade.id);
        assert_eq!(order_ids.entry, "entry-id");
        assert_eq!(order_ids.stop, "stop-id");
        assert_eq!(order_ids.target, "target-id");

        let (status, orders, sync_log) = broker
            .sync_trade(&trade, &account)
            .expect("sync should succeed");
        assert_eq!(status, Status::Submitted);
        assert!(orders.is_empty());
        assert_eq!(sync_log.log, "synced");

        let (_order, close_log) = broker
            .close_trade(&trade, &account)
            .expect("close should succeed");
        assert_eq!(close_log.log, "closed");

        assert!(broker.cancel_trade(&trade, &account).is_ok());
        assert_eq!(
            broker
                .modify_stop(&trade, &account, dec!(99))
                .expect("modify stop"),
            "stop-id"
        );
        assert_eq!(
            broker
                .modify_target(&trade, &account, dec!(101))
                .expect("modify target"),
            "target-id"
        );
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
        (account, trade)
    }

    fn permissive_advisory_thresholds() -> core::services::AdvisoryThresholds {
        core::services::AdvisoryThresholds {
            sector_limit_pct: dec!(100),
            asset_class_limit_pct: dec!(100),
            single_position_limit_pct: dec!(100),
        }
    }

    fn blocking_advisory_thresholds() -> core::services::AdvisoryThresholds {
        core::services::AdvisoryThresholds {
            sector_limit_pct: dec!(10),
            asset_class_limit_pct: dec!(10),
            single_position_limit_pct: dec!(10),
        }
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
    fn build_surfaces_advisory_threshold_read_error() {
        let mut trust = TrustFacade::new(
            Box::new(crate::test_support::ReadFailureFactory::advisory()),
            Box::<AlpacaBroker>::default(),
        );
        let mut io = StubDialogIo {
            select_result: Ok(None),
            confirm_result: Ok(true),
        };

        let builder = FundingDialogBuilder {
            account: Some(Account::default()),
            trade: Some(Trade::default()),
            result: None,
        }
        .build_with_io(&mut trust, &mut io);

        let err = builder
            .result
            .expect("result")
            .expect_err("advisory read error should be surfaced");
        assert_eq!(err.to_string(), "advisory read failed");
    }

    #[test]
    fn build_cancels_when_advisory_warning_is_not_confirmed() {
        let mut trust = test_trust();
        let (account, trade) = seed_new_trade(&mut trust, "fund-warning");
        trust
            .configure_advisory_thresholds(account.id, permissive_advisory_thresholds())
            .expect("thresholds");
        let mut io = StubDialogIo {
            select_result: Ok(None),
            confirm_result: Ok(false),
        };

        let builder = FundingDialogBuilder {
            account: Some(account),
            trade: Some(trade),
            result: None,
        }
        .build_with_io(&mut trust, &mut io);

        let err = builder
            .result
            .expect("result")
            .expect_err("warning should require confirmation");
        assert!(err
            .to_string()
            .contains("Funding canceled by user after advisory"));
    }

    #[test]
    fn build_blocks_trade_when_advisory_hard_limit_is_exceeded() {
        let mut trust = test_trust();
        let (account, trade) = seed_new_trade(&mut trust, "fund-block");
        trust
            .configure_advisory_thresholds(account.id, blocking_advisory_thresholds())
            .expect("thresholds");
        let mut io = StubDialogIo {
            select_result: Ok(None),
            confirm_result: Ok(true),
        };

        let builder = FundingDialogBuilder {
            account: Some(account),
            trade: Some(trade),
            result: None,
        }
        .build_with_io(&mut trust, &mut io);

        let err = builder
            .result
            .expect("result")
            .expect_err("hard advisory limits should block funding");
        assert!(err.to_string().contains("Trade blocked by advisory limits"));
    }

    #[test]
    fn build_funds_trade_when_advisory_warning_is_confirmed_and_display_success() {
        let mut trust = test_trust();
        let (account, trade) = seed_new_trade(&mut trust, "fund-confirmed");
        trust
            .configure_advisory_thresholds(account.id, permissive_advisory_thresholds())
            .expect("thresholds");
        let mut io = StubDialogIo {
            select_result: Ok(None),
            confirm_result: Ok(true),
        };

        let builder = FundingDialogBuilder {
            account: Some(account),
            trade: Some(trade),
            result: None,
        }
        .build_with_io(&mut trust, &mut io);

        assert!(builder.result.as_ref().expect("result").as_ref().is_ok());
        builder.display();
    }

    #[test]
    fn build_reaches_ok_advisory_branch_before_funding_validation_error() {
        let mut trust = test_trust();
        let account = trust
            .create_account(
                "fund-ok-branch",
                "test",
                Environment::Paper,
                dec!(20),
                dec!(10),
            )
            .expect("account");
        let vehicle = trust
            .create_trading_vehicle("ZERO", None, &TradingVehicleCategory::Stock, "alpaca")
            .expect("vehicle");
        let trade = Trade {
            account_id: account.id,
            trading_vehicle: vehicle,
            ..Trade::default()
        };
        let mut io = StubDialogIo {
            select_result: Ok(None),
            confirm_result: Ok(false),
        };

        let builder = FundingDialogBuilder {
            account: Some(account),
            trade: Some(trade),
            result: None,
        }
        .build_with_io(&mut trust, &mut io);

        let _ = builder
            .result
            .expect("result")
            .expect_err("invalid zero-notional trade should not fund");
    }

    #[test]
    fn build_funds_trade_when_advisory_is_ok() {
        let mut trust = test_trust_with_broker(Box::new(SubmitOkBroker));
        let account = trust
            .create_account(
                "fund-ok-advisory",
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
                dec!(50_000),
                &Currency::USD,
            )
            .expect("deposit");

        let hedge_vehicle = trust
            .create_trading_vehicle("MSFT", None, &TradingVehicleCategory::Stock, "alpaca")
            .expect("hedge vehicle");
        let hedge = trust
            .create_trade(
                DraftTrade {
                    account: account.clone(),
                    trading_vehicle: hedge_vehicle,
                    quantity: 100.into(),
                    currency: Currency::USD,
                    category: TradeCategory::Long,
                    thesis: None,
                    sector: Some("Software".to_string()),
                    asset_class: Some("Stocks".to_string()),
                    context: None,
                },
                dec!(95),
                dec!(100),
                dec!(110),
            )
            .expect("hedge trade");
        let (funded_hedge, _, _, _) = trust.fund_trade(&hedge).expect("hedge funding");
        trust
            .submit_trade(&funded_hedge)
            .expect("hedge submission should create open exposure");

        let proposal_vehicle = trust
            .create_trading_vehicle("AAPL", None, &TradingVehicleCategory::Stock, "alpaca")
            .expect("proposal vehicle");
        let proposal = trust
            .create_trade(
                DraftTrade {
                    account: account.clone(),
                    trading_vehicle: proposal_vehicle,
                    quantity: 10.into(),
                    currency: Currency::USD,
                    category: TradeCategory::Long,
                    thesis: None,
                    sector: Some("Healthcare".to_string()),
                    asset_class: Some("Alternatives".to_string()),
                    context: None,
                },
                dec!(95),
                dec!(100),
                dec!(110),
            )
            .expect("proposal trade");
        let mut io = StubDialogIo {
            select_result: Ok(None),
            confirm_result: Ok(false),
        };

        let builder = FundingDialogBuilder {
            account: Some(account),
            trade: Some(proposal),
            result: None,
        }
        .build_with_io(&mut trust, &mut io);

        assert!(builder.result.expect("result").is_ok());
    }

    #[test]
    fn account_wrapper_handles_search_error_and_stub_confirm_returns_default_false() {
        let mut trust = test_trust();
        scripted_reset();

        let builder = FundingDialogBuilder::new().account(&mut trust);
        assert!(builder.account.is_none());

        let mut io = StubDialogIo {
            select_result: Ok(None),
            confirm_result: Ok(true),
        };
        assert!(
            crate::dialogs::DialogIo::confirm(&mut io, "continue?", false)
                .expect("confirm should return")
        );
        assert!(
            !crate::dialogs::DialogIo::confirm(&mut io, "continue?", true)
                .expect("confirm should fall back")
        );

        scripted_reset();
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
    fn search_with_io_stores_trade_read_errors() {
        let mut trust = TrustFacade::new(
            Box::new(crate::test_support::ReadFailureFactory::trades()),
            Box::<AlpacaBroker>::default(),
        );
        let mut io = StubDialogIo {
            select_result: Ok(None),
            confirm_result: Ok(true),
        };

        let builder = FundingDialogBuilder {
            account: Some(Account {
                id: Uuid::new_v4(),
                ..Account::default()
            }),
            trade: None,
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
