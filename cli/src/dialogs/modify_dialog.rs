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
use crate::views::{OrderView, TradeBalanceView, TradeView};
use core::TrustFacade;
use model::{Account, Status, Trade};
use rust_decimal::Decimal;
use std::error::Error;
use std::io::ErrorKind;

type ModifyDialogBuilderResult = Option<Result<Trade, Box<dyn Error>>>;

pub struct ModifyDialogBuilder {
    account: Option<Account>,
    trade: Option<Trade>,
    new_price: Option<Decimal>,
    result: ModifyDialogBuilderResult,
}

impl ModifyDialogBuilder {
    pub fn new() -> Self {
        ModifyDialogBuilder {
            account: None,
            trade: None,
            new_price: None,
            result: None,
        }
    }

    pub fn build_stop(mut self, trust: &mut TrustFacade) -> ModifyDialogBuilder {
        let trade = match dialog_helpers::require(
            self.trade.clone(),
            ErrorKind::InvalidInput,
            "No trade selected for stop update",
        ) {
            Ok(trade) => trade,
            Err(error) => {
                self.result = Some(Err(error));
                return self;
            }
        };
        let account = match dialog_helpers::require(
            self.account.clone(),
            ErrorKind::InvalidInput,
            "No account selected for stop update",
        ) {
            Ok(account) => account,
            Err(error) => {
                self.result = Some(Err(error));
                return self;
            }
        };
        let stop_price = match dialog_helpers::require(
            self.new_price,
            ErrorKind::InvalidInput,
            "No stop price found, did you forget to call stop_price?",
        ) {
            Ok(stop_price) => stop_price,
            Err(error) => {
                self.result = Some(Err(error));
                return self;
            }
        };

        match trust.modify_stop(&trade, &account, stop_price) {
            Ok(trade) => self.result = Some(Ok(trade)),
            Err(error) => self.result = Some(Err(error)),
        }
        self
    }

    pub fn build_target(mut self, trust: &mut TrustFacade) -> ModifyDialogBuilder {
        let trade = match dialog_helpers::require(
            self.trade.clone(),
            ErrorKind::InvalidInput,
            "No trade selected for target update",
        ) {
            Ok(trade) => trade,
            Err(error) => {
                self.result = Some(Err(error));
                return self;
            }
        };
        let account = match dialog_helpers::require(
            self.account.clone(),
            ErrorKind::InvalidInput,
            "No account selected for target update",
        ) {
            Ok(account) => account,
            Err(error) => {
                self.result = Some(Err(error));
                return self;
            }
        };
        let target_price = match dialog_helpers::require(
            self.new_price,
            ErrorKind::InvalidInput,
            "No target price found, did you forget to call stop_price?",
        ) {
            Ok(target_price) => target_price,
            Err(error) => {
                self.result = Some(Err(error));
                return self;
            }
        };

        match trust.modify_target(&trade, &account, target_price) {
            Ok(trade) => self.result = Some(Ok(trade)),
            Err(error) => self.result = Some(Err(error)),
        }
        self
    }

    pub fn display(self) {
        match self
            .result
            .expect("No result found, did you forget to call search?")
        {
            Ok(trade) => {
                println!("Trade updated:");
                let account_name = self
                    .account
                    .as_ref()
                    .map_or("<unknown account>", |account| account.name.as_str());
                TradeView::display(&trade, account_name);

                TradeBalanceView::display(&trade.balance);

                println!("Stop:");
                OrderView::display(trade.safety_stop);

                println!("Target:");
                OrderView::display(trade.target);
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

    pub fn search(mut self, trust: &mut TrustFacade) -> Self {
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
        let mut io = ConsoleDialogIo::default();
        match trades {
            Ok(trades) => match dialog_helpers::select_from_list(
                &mut io,
                "Trade:",
                &trades,
                "No filled trade found for this account",
                "Trade selection was canceled",
            ) {
                Ok(trade) => {
                    TradeView::display(&trade, account.name.as_str());
                    self.trade = Some(trade);
                }
                Err(error) => self.result = Some(Err(error)),
            },
            Err(error) => self.result = Some(Err(error)),
        }

        self
    }

    pub fn new_price(mut self) -> Self {
        let mut io = ConsoleDialogIo::default();
        self = self.new_price_with_io(&mut io);
        self
    }

    pub fn new_price_with_io(mut self, io: &mut dyn DialogIo) -> Self {
        match io.input_text("New price", false) {
            Ok(raw) => match raw.parse::<Decimal>() {
                Ok(value) => self.new_price = Some(value),
                Err(_) => println!("Please enter a valid number."),
            },
            Err(error) => println!("Error reading new price: {error}"),
        }
        self
    }
}

#[cfg(test)]
mod tests {
    use super::ModifyDialogBuilder;
    use crate::dialogs::io::{scripted_push_input, scripted_push_select, scripted_reset};
    use crate::dialogs::DialogIo;
    use alpaca_broker::AlpacaBroker;
    use core::TrustFacade;
    use db_sqlite::SqliteDatabase;
    use model::{
        Account, Broker, BrokerKind, BrokerLog, Currency, DraftTrade, Environment, Order, OrderIds,
        OrderStatus, Status, Trade, TradeCategory, TradingVehicleCategory, TransactionCategory,
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

    struct ModifyOkBroker;

    impl Broker for ModifyOkBroker {
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
                        filled_at: Some(chrono::Utc::now().naive_utc()),
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
            _trade: &Trade,
            _account: &Account,
        ) -> Result<(Order, BrokerLog), Box<dyn std::error::Error>> {
            Err("close not supported".into())
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
            Ok("modified-stop".to_string())
        }

        fn modify_target(
            &self,
            _trade: &Trade,
            _account: &Account,
            _new_price: Decimal,
        ) -> Result<String, Box<dyn std::error::Error>> {
            Ok("modified-target".to_string())
        }
    }

    fn test_trust_with_modify_broker() -> TrustFacade {
        TrustFacade::new(
            Box::new(SqliteDatabase::new_in_memory()),
            Box::new(ModifyOkBroker),
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

    struct InputErrorIo;

    impl DialogIo for InputErrorIo {
        fn select_index(
            &mut self,
            _prompt: &str,
            _labels: &[String],
            _default: usize,
        ) -> Result<Option<usize>, IoError> {
            Ok(None)
        }

        fn confirm(&mut self, _prompt: &str, _default: bool) -> Result<bool, IoError> {
            Ok(false)
        }

        fn input_text(&mut self, _prompt: &str, _allow_empty: bool) -> Result<String, IoError> {
            Err(IoError::new(ErrorKind::BrokenPipe, "input failed"))
        }
    }

    #[test]
    fn new_starts_with_empty_state() {
        let builder = ModifyDialogBuilder::new();
        assert!(builder.account.is_none());
        assert!(builder.trade.is_none());
        assert!(builder.new_price.is_none());
        assert!(builder.result.is_none());
    }

    #[test]
    fn build_stop_returns_error_when_trade_is_missing() {
        let mut trust = test_trust();
        let builder = ModifyDialogBuilder::new().build_stop(&mut trust);
        let err = builder
            .result
            .expect("result should be set")
            .expect_err("missing trade should fail");
        assert!(err
            .to_string()
            .contains("No trade selected for stop update"));
    }

    #[test]
    fn build_target_returns_error_when_trade_is_missing() {
        let mut trust = test_trust();
        let builder = ModifyDialogBuilder::new().build_target(&mut trust);
        let err = builder
            .result
            .expect("result should be set")
            .expect_err("missing trade should fail");
        assert!(err
            .to_string()
            .contains("No trade selected for target update"));
    }

    #[test]
    fn build_stop_returns_error_when_account_is_missing() {
        let mut trust = test_trust();
        let builder = ModifyDialogBuilder {
            account: None,
            trade: Some(Trade::default()),
            new_price: Some(dec!(10)),
            result: None,
        }
        .build_stop(&mut trust);

        let err = builder
            .result
            .expect("result should be set")
            .expect_err("missing account should fail");
        assert!(err
            .to_string()
            .contains("No account selected for stop update"));
    }

    #[test]
    fn build_target_returns_error_when_account_is_missing() {
        let mut trust = test_trust();
        let builder = ModifyDialogBuilder {
            account: None,
            trade: Some(Trade::default()),
            new_price: Some(dec!(10)),
            result: None,
        }
        .build_target(&mut trust);

        let err = builder
            .result
            .expect("result should be set")
            .expect_err("missing account should fail");
        assert!(err
            .to_string()
            .contains("No account selected for target update"));
    }

    #[test]
    fn build_stop_returns_error_when_price_is_missing() {
        let mut trust = test_trust();
        let builder = ModifyDialogBuilder {
            account: Some(Account::default()),
            trade: Some(Trade::default()),
            new_price: None,
            result: None,
        }
        .build_stop(&mut trust);

        let err = builder
            .result
            .expect("result should be set")
            .expect_err("missing stop should fail");
        assert!(err
            .to_string()
            .contains("No stop price found, did you forget to call stop_price?"));
    }

    #[test]
    fn build_target_returns_error_when_price_is_missing() {
        let mut trust = test_trust();
        let builder = ModifyDialogBuilder {
            account: Some(Account::default()),
            trade: Some(Trade::default()),
            new_price: None,
            result: None,
        }
        .build_target(&mut trust);

        let err = builder
            .result
            .expect("result should be set")
            .expect_err("missing target should fail");
        assert!(err
            .to_string()
            .contains("No target price found, did you forget to call stop_price?"));
    }

    #[test]
    fn build_stop_and_target_call_facade_when_required_fields_are_present() {
        let mut trust = test_trust();
        let account = Account::default();
        let trade = Trade::default();

        let stop = ModifyDialogBuilder {
            account: Some(account.clone()),
            trade: Some(trade.clone()),
            new_price: Some(dec!(9)),
            result: None,
        }
        .build_stop(&mut trust);
        assert!(stop.result.is_some());

        let target = ModifyDialogBuilder {
            account: Some(account),
            trade: Some(trade),
            new_price: Some(dec!(11)),
            result: None,
        }
        .build_target(&mut trust);
        assert!(target.result.is_some());
    }

    #[test]
    fn build_stop_and_target_modify_persisted_filled_trade() {
        let mut trust = test_trust_with_modify_broker();
        let (account, filled) = seed_filled_trade(&mut trust, "modify-build-success");

        let stop = ModifyDialogBuilder {
            account: Some(account.clone()),
            trade: Some(filled),
            new_price: Some(dec!(96)),
            result: None,
        }
        .build_stop(&mut trust);
        let stop_trade = stop
            .result
            .expect("stop result should be set")
            .expect("stop modify should succeed");
        assert_eq!(stop_trade.safety_stop.unit_price, dec!(96));
        assert_eq!(
            stop_trade.safety_stop.broker_order_id.as_deref(),
            Some("modified-stop")
        );

        let target = ModifyDialogBuilder {
            account: Some(account),
            trade: Some(stop_trade),
            new_price: Some(dec!(111)),
            result: None,
        }
        .build_target(&mut trust);
        let target_trade = target
            .result
            .expect("target result should be set")
            .expect("target modify should succeed");
        assert_eq!(target_trade.target.unit_price, dec!(111));
        assert_eq!(
            target_trade.target.broker_order_id.as_deref(),
            Some("modified-target")
        );
    }

    #[test]
    fn search_returns_error_when_account_is_missing() {
        let mut trust = test_trust();
        let builder = ModifyDialogBuilder::new().search(&mut trust);
        let err = builder
            .result
            .expect("result should be set")
            .expect_err("missing account should fail");
        assert!(err.to_string().contains("No account selected"));
    }

    #[test]
    fn search_returns_error_when_account_has_no_filled_trades() {
        let mut trust = test_trust();
        let account = trust
            .create_account(
                "modify-empty-search",
                "desc",
                Environment::Paper,
                dec!(20),
                dec!(10),
            )
            .expect("account");
        scripted_reset();

        let builder = ModifyDialogBuilder {
            account: Some(account),
            trade: None,
            new_price: None,
            result: None,
        }
        .search(&mut trust);

        let err = builder
            .result
            .expect("result should be set")
            .expect_err("empty search should fail");
        assert!(err
            .to_string()
            .contains("No filled trade found for this account"));
        scripted_reset();
    }

    #[test]
    fn search_stores_trade_read_errors() {
        let mut trust = TrustFacade::new(
            Box::new(crate::test_support::ReadFailureFactory::trades()),
            Box::<AlpacaBroker>::default(),
        );

        let builder = ModifyDialogBuilder {
            account: Some(Account {
                id: Uuid::new_v4(),
                ..Account::default()
            }),
            trade: None,
            new_price: None,
            result: None,
        }
        .search(&mut trust);

        let err = builder
            .result
            .expect("trade read error should set result")
            .expect_err("trade read should fail");
        assert!(err.to_string().contains("trade read failed"));
    }

    #[test]
    fn search_selects_filled_trade_and_reports_canceled_selection() {
        let mut trust = test_trust_with_modify_broker();
        let (account, filled) = seed_filled_trade(&mut trust, "modify-search-select");

        scripted_reset();
        scripted_push_select(Ok(Some(0)));
        let selected = ModifyDialogBuilder {
            account: Some(account.clone()),
            trade: None,
            new_price: None,
            result: None,
        }
        .search(&mut trust);
        assert_eq!(selected.trade.expect("selected trade").id, filled.id);
        assert!(selected.result.is_none());

        scripted_reset();
        scripted_push_select(Ok(None));
        let canceled = ModifyDialogBuilder {
            account: Some(account.clone()),
            trade: None,
            new_price: None,
            result: None,
        }
        .search(&mut trust);
        let err = canceled
            .result
            .expect("canceled selection should set result")
            .expect_err("canceled selection should fail");
        assert!(err.to_string().contains("Trade selection was canceled"));

        scripted_reset();
        scripted_push_select(Err(IoError::new(ErrorKind::BrokenPipe, "select failed")));
        let errored = ModifyDialogBuilder {
            account: Some(account),
            trade: None,
            new_price: None,
            result: None,
        }
        .search(&mut trust);
        let err = errored
            .result
            .expect("failed selection should set result")
            .expect_err("failed selection should fail");
        assert!(err.to_string().contains("Trade selection was canceled"));
        scripted_reset();
    }

    #[test]
    fn modify_ok_broker_aux_methods_have_expected_contracts() {
        let broker = ModifyOkBroker;
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

        let close = broker
            .close_trade(&trade, &account)
            .expect_err("close should be unsupported");
        assert!(close.to_string().contains("close not supported"));
        let cancel = broker
            .cancel_trade(&trade, &account)
            .expect_err("cancel should be unsupported");
        assert!(cancel.to_string().contains("cancel not supported"));
        assert_eq!(
            broker
                .modify_stop(&trade, &account, dec!(1))
                .expect("stop modify"),
            "modified-stop"
        );
        assert_eq!(
            broker
                .modify_target(&trade, &account, dec!(1))
                .expect("target modify"),
            "modified-target"
        );
    }

    #[test]
    fn display_handles_error_result() {
        let builder = ModifyDialogBuilder {
            account: None,
            trade: None,
            new_price: None,
            result: Some(Err("synthetic failure".into())),
        };
        builder.display();
    }

    #[test]
    fn display_handles_success_result() {
        let builder = ModifyDialogBuilder {
            account: Some(Account {
                name: "paper".to_string(),
                ..Account::default()
            }),
            trade: Some(Trade::default()),
            new_price: Some(dec!(99)),
            result: Some(Ok(Trade::default())),
        };
        builder.display();
    }

    #[test]
    fn new_price_with_io_and_wrapper_cover_success_and_invalid() {
        struct ScriptedIo(Vec<String>);
        impl DialogIo for ScriptedIo {
            fn select_index(
                &mut self,
                _prompt: &str,
                _labels: &[String],
                _default: usize,
            ) -> Result<Option<usize>, std::io::Error> {
                Ok(None)
            }
            fn confirm(&mut self, _prompt: &str, _default: bool) -> Result<bool, std::io::Error> {
                Ok(false)
            }
            fn input_text(
                &mut self,
                _prompt: &str,
                _allow_empty: bool,
            ) -> Result<String, std::io::Error> {
                Ok(self.0.remove(0))
            }
        }

        let mut io = ScriptedIo(vec!["99.1".to_string()]);
        let parsed = ModifyDialogBuilder::new().new_price_with_io(&mut io);
        assert_eq!(parsed.new_price, Some(dec!(99.1)));

        let mut io = ScriptedIo(vec!["abc".to_string()]);
        let unchanged = ModifyDialogBuilder {
            account: None,
            trade: None,
            new_price: Some(dec!(5)),
            result: None,
        }
        .new_price_with_io(&mut io);
        assert_eq!(unchanged.new_price, Some(dec!(5)));
        assert!(io
            .select_index("unused", &[], 0)
            .expect("select returns")
            .is_none());
        assert!(!io.confirm("unused", true).expect("confirm returns"));

        let mut error_io = InputErrorIo;
        let errored = ModifyDialogBuilder {
            account: None,
            trade: None,
            new_price: Some(dec!(7)),
            result: None,
        }
        .new_price_with_io(&mut error_io);
        assert_eq!(errored.new_price, Some(dec!(7)));
        assert!(error_io
            .select_index("unused", &[], 0)
            .expect("select returns")
            .is_none());
        assert!(!error_io.confirm("unused", true).expect("confirm returns"));

        scripted_reset();
        scripted_push_input(Ok("33.3".to_string()));
        let wrapped = ModifyDialogBuilder::new().new_price();
        assert_eq!(wrapped.new_price, Some(dec!(33.3)));
        scripted_reset();
    }

    #[test]
    fn account_wrapper_handles_search_error() {
        let mut trust = test_trust();
        scripted_reset();

        let builder = ModifyDialogBuilder::new().account(&mut trust);

        assert!(builder.account.is_none());
        scripted_reset();
    }

    #[test]
    fn account_wrapper_uses_default_console_io_in_tests() {
        let mut trust = test_trust();
        let account = trust
            .create_account(
                "modify-wrapper",
                "desc",
                Environment::Paper,
                dec!(20),
                dec!(10),
            )
            .expect("account");

        scripted_reset();
        scripted_push_select(Ok(Some(0)));
        let builder = ModifyDialogBuilder::new().account(&mut trust);
        assert_eq!(
            builder.account.as_ref().expect("selected account").id,
            account.id
        );
        scripted_reset();
    }
}
