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
    use crate::dialogs::io::{scripted_push_select, scripted_reset};
    use alpaca_broker::AlpacaBroker;
    use chrono::Utc;
    use core::TrustFacade;
    use db_sqlite::SqliteDatabase;
    use model::{
        Account, Broker, BrokerKind, BrokerLog, Currency, DistributionResult, DraftTrade, Order,
        OrderIds, OrderStatus, Status, Trade, TradeBalance, TradeCategory, TradingVehicleCategory,
        TransactionCategory,
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

    struct CloseOkBroker;

    impl Broker for CloseOkBroker {
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
                    entry: Uuid::new_v4().to_string(),
                    stop: Uuid::new_v4().to_string(),
                    target: Uuid::new_v4().to_string(),
                },
            ))
        }

        fn sync_trade(
            &self,
            trade: &Trade,
            _account: &Account,
        ) -> Result<(Status, Vec<Order>, BrokerLog), Box<dyn std::error::Error>> {
            Ok((
                Status::Filled,
                vec![
                    Order {
                        id: trade.entry.id,
                        broker_order_id: trade.entry.broker_order_id.clone(),
                        filled_quantity: trade.entry.quantity,
                        average_filled_price: Some(dec!(100)),
                        status: OrderStatus::Filled,
                        filled_at: Some(Utc::now().naive_utc()),
                        ..Order::default()
                    },
                    Order {
                        id: trade.target.id,
                        broker_order_id: trade.target.broker_order_id.clone(),
                        status: OrderStatus::Accepted,
                        ..Order::default()
                    },
                    Order {
                        id: trade.safety_stop.id,
                        broker_order_id: trade.safety_stop.broker_order_id.clone(),
                        status: OrderStatus::Held,
                        ..Order::default()
                    },
                ],
                BrokerLog {
                    trade_id: trade.id,
                    log: "filled".to_string(),
                    ..BrokerLog::default()
                },
            ))
        }

        fn close_trade(
            &self,
            trade: &Trade,
            _account: &Account,
        ) -> Result<(Order, BrokerLog), Box<dyn std::error::Error>> {
            let mut target = trade.target.clone();
            target.broker_order_id = Some("manual-close-target".to_string());
            target.status = OrderStatus::Filled;
            target.filled_quantity = target.quantity;
            target.average_filled_price = Some(target.unit_price);
            target.filled_at = Some(Utc::now().naive_utc());
            Ok((
                target,
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
            Err("cancel not supported".into())
        }

        fn modify_stop(
            &self,
            _trade: &Trade,
            _account: &Account,
            _new_stop_price: Decimal,
        ) -> Result<String, Box<dyn std::error::Error>> {
            Err("modify stop not supported".into())
        }

        fn modify_target(
            &self,
            _trade: &Trade,
            _account: &Account,
            _new_price: Decimal,
        ) -> Result<String, Box<dyn std::error::Error>> {
            Err("modify target not supported".into())
        }
    }

    fn test_trust_with_close_broker() -> TrustFacade {
        TrustFacade::new(
            Box::new(SqliteDatabase::new_in_memory()),
            Box::new(CloseOkBroker),
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
    fn close_ok_broker_aux_methods_have_expected_contracts() {
        let broker = CloseOkBroker;
        let trade = Trade::default();
        let account = Account::default();

        assert_eq!(broker.kind(), BrokerKind::Alpaca);
        let (log, ids) = broker
            .submit_trade(&trade, &account)
            .expect("submit should succeed");
        assert_eq!(log.trade_id, trade.id);
        assert!(!ids.entry.is_empty());
        assert!(!ids.stop.is_empty());
        assert!(!ids.target.is_empty());
        let (status, orders, log) = broker.sync_trade(&trade, &account).expect("sync");
        assert_eq!(status, Status::Filled);
        assert_eq!(orders.len(), 3);
        assert_eq!(log.trade_id, trade.id);
        let (order, log) = broker
            .close_trade(&trade, &account)
            .expect("close should succeed");
        assert_eq!(order.id, trade.target.id);
        assert_eq!(log.trade_id, trade.id);

        let cancel = broker
            .cancel_trade(&trade, &account)
            .expect_err("cancel should be unsupported");
        assert!(cancel.to_string().contains("cancel not supported"));
        let stop = broker
            .modify_stop(&trade, &account, dec!(1))
            .expect_err("stop modify should be unsupported");
        assert!(stop.to_string().contains("modify stop not supported"));
        let target = broker
            .modify_target(&trade, &account, dec!(1))
            .expect_err("target modify should be unsupported");
        assert!(target.to_string().contains("modify target not supported"));
    }

    #[test]
    fn build_calls_close_paths_for_selected_trade() {
        let mut trust = test_trust();
        let filled = Trade {
            status: Status::Filled,
            ..Trade::default()
        };

        let manual = CloseDialogBuilder {
            account: None,
            trade: Some(filled.clone()),
            auto_distribute: false,
            result: None,
        }
        .build(&mut trust);
        assert!(manual.result.is_some());

        let distributed = CloseDialogBuilder {
            account: None,
            trade: Some(filled),
            auto_distribute: true,
            result: None,
        }
        .build(&mut trust);
        assert!(distributed.result.is_some());
    }

    #[test]
    fn stub_dialog_io_confirm_default_is_false() {
        let mut io = StubDialogIo {
            select_result: Ok(None),
        };

        assert!(
            !crate::dialogs::DialogIo::confirm(&mut io, "continue?", true)
                .expect("confirm should return")
        );
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

    #[test]
    fn display_handles_success_result_without_distribution() {
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
    }
}
