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
    use model::{Account, Environment, Trade};
    use rust_decimal_macros::dec;
    use uuid::Uuid;

    fn test_trust() -> TrustFacade {
        let path = std::env::temp_dir().join(format!("trust-test-{}.db", Uuid::new_v4()));
        let db = SqliteDatabase::new(path.to_str().expect("valid temp db path"));
        TrustFacade::new(Box::new(db), Box::<AlpacaBroker>::default())
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

        scripted_reset();
        scripted_push_input(Ok("33.3".to_string()));
        let wrapped = ModifyDialogBuilder::new().new_price();
        assert_eq!(wrapped.new_price, Some(dec!(33.3)));
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
