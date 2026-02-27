//! Trade creation dialog - UI interaction module
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

use crate::{
    dialogs::{
        io::ConsoleDialogIo, io::DialogIo, AccountSearchDialog, TradingVehicleSearchDialogBuilder,
    },
    views::TradeBalanceView,
    views::TradeView,
};
use core::TrustFacade;
use model::{Account, Currency, DraftTrade, Trade, TradeCategory, TradingVehicle};
use rust_decimal::Decimal;
use std::error::Error;

pub struct TradeDialogBuilder {
    account: Option<Account>,
    trading_vehicle: Option<TradingVehicle>,
    category: Option<TradeCategory>,
    entry_price: Option<Decimal>,
    stop_price: Option<Decimal>,
    currency: Option<Currency>,
    quantity: Option<i64>,
    target_price: Option<Decimal>,
    thesis: Option<String>,
    sector: Option<String>,
    asset_class: Option<String>,
    context: Option<String>,
    result: Option<Result<Trade, Box<dyn Error>>>,
}

impl TradeDialogBuilder {
    pub fn new() -> Self {
        TradeDialogBuilder {
            account: None,
            trading_vehicle: None,
            category: None,
            entry_price: None,
            stop_price: None,
            currency: None,
            quantity: None,
            target_price: None,
            thesis: None,
            sector: None,
            asset_class: None,
            context: None,
            result: None,
        }
    }

    pub fn build(mut self, trust: &mut TrustFacade) -> TradeDialogBuilder {
        let trading_vehicle = self
            .trading_vehicle
            .clone()
            .expect("Did you forget to specify trading vehicle");

        let draft = DraftTrade {
            account: self.account.clone().unwrap(),
            trading_vehicle,
            quantity: self.quantity.unwrap(),
            currency: self.currency.unwrap(),
            category: self.category.unwrap(),
            thesis: self.thesis.clone(),
            sector: self.sector.clone(),
            asset_class: self.asset_class.clone(),
            context: self.context.clone(),
        };

        self.result = Some(trust.create_trade(
            draft,
            self.stop_price.unwrap(),
            self.entry_price.unwrap(),
            self.target_price.unwrap(),
        ));
        self
    }

    pub fn display(self) {
        match self
            .result
            .expect("No result found, did you forget to call build?")
        {
            Ok(trade) => {
                TradeView::display(&trade, &self.account.unwrap().name);
                TradeBalanceView::display(&trade.balance);
            }
            Err(error) => println!("Error creating account: {error:?}"),
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

    pub fn trading_vehicle(mut self, trust: &mut TrustFacade) -> Self {
        let tv = TradingVehicleSearchDialogBuilder::new()
            .search(trust)
            .build();
        match tv {
            Ok(tv) => self.trading_vehicle = Some(tv),
            Err(error) => println!("Error searching trading vehicle: {error:?}"),
        }
        self
    }

    pub fn category(mut self) -> Self {
        let mut io = ConsoleDialogIo::default();
        self = self.category_with_io(&mut io);
        self
    }

    pub fn category_with_io(mut self, io: &mut dyn DialogIo) -> Self {
        let available_categories = TradeCategory::all();
        let labels: Vec<String> = available_categories
            .iter()
            .map(ToString::to_string)
            .collect();
        match io.select_index("Category:", &labels, 0) {
            Ok(Some(index)) => {
                self.category = available_categories.get(index).copied();
            }
            Ok(None) => {}
            Err(error) => println!("Error selecting category: {error}"),
        }
        self
    }

    pub fn entry_price(mut self) -> Self {
        let mut io = ConsoleDialogIo::default();
        self = self.entry_price_with_io(&mut io);
        self
    }

    pub fn entry_price_with_io(mut self, io: &mut dyn DialogIo) -> Self {
        match io.input_text("Entry price", false) {
            Ok(raw) => match raw.parse::<Decimal>() {
                Ok(value) => self.entry_price = Some(value),
                Err(_) => println!("Please enter a valid number."),
            },
            Err(error) => println!("Error reading entry price: {error}"),
        }
        self
    }

    pub fn stop_price(mut self) -> Self {
        let mut io = ConsoleDialogIo::default();
        self = self.stop_price_with_io(&mut io);
        self
    }

    pub fn stop_price_with_io(mut self, io: &mut dyn DialogIo) -> Self {
        match io.input_text("Stop price", false) {
            Ok(raw) => match raw.parse::<Decimal>() {
                Ok(value) => self.stop_price = Some(value),
                Err(_) => println!("Please enter a valid number."),
            },
            Err(error) => println!("Error reading stop price: {error}"),
        }
        self
    }

    pub fn currency(mut self, trust: &mut TrustFacade) -> Self {
        let mut io = ConsoleDialogIo::default();
        self = self.currency_with_io(trust, &mut io);
        self
    }

    pub fn currency_with_io(mut self, trust: &mut TrustFacade, io: &mut dyn DialogIo) -> Self {
        let currencies: Vec<Currency> = trust
            .search_all_balances(self.account.clone().unwrap().id)
            .unwrap()
            .into_iter()
            .map(|balance| balance.currency)
            .collect();
        let labels: Vec<String> = currencies.iter().map(ToString::to_string).collect();
        match io.select_index("Currency:", &labels, 0) {
            Ok(Some(index)) => self.currency = currencies.get(index).copied(),
            Ok(None) => {}
            Err(error) => println!("Error selecting currency: {error}"),
        }
        self
    }

    pub fn quantity(mut self, trust: &mut TrustFacade) -> Self {
        let mut io = ConsoleDialogIo::default();
        self = self.quantity_with_io(trust, &mut io);
        self
    }

    pub fn quantity_with_io(mut self, trust: &mut TrustFacade, io: &mut dyn DialogIo) -> Self {
        let (base_quantity, level_multiplier, maximum) = match trust
            .calculate_level_adjusted_quantity(
                self.account.clone().unwrap().id,
                self.entry_price.unwrap(),
                self.stop_price.unwrap(),
                &self.currency.unwrap(),
            ) {
            Ok(sizing) => (
                sizing.base_quantity,
                sizing.level_multiplier,
                sizing.final_quantity,
            ),
            Err(error) => {
                println!("Error calculating maximum quantity {error}");
                (0, rust_decimal::Decimal::ONE, 0)
            }
        };

        println!("Base quantity (rules only): {base_quantity}");
        println!("Level multiplier: {level_multiplier}x");
        println!("Maximum quantity (level-adjusted): {maximum}");

        match io.input_text("Quantity", false) {
            Ok(raw) => match raw.parse::<i64>() {
                Ok(parsed) if parsed > maximum => {
                    println!("Please enter a number below your maximum allowed");
                }
                Ok(0) => println!("Please enter a number above 0"),
                Ok(parsed) => self.quantity = Some(parsed),
                Err(_) => println!("Please enter a valid number."),
            },
            Err(error) => println!("Error reading quantity: {error}"),
        }
        self
    }

    pub fn target_price(mut self) -> Self {
        let mut io = ConsoleDialogIo::default();
        self = self.target_price_with_io(&mut io);
        self
    }

    pub fn target_price_with_io(mut self, io: &mut dyn DialogIo) -> Self {
        match io.input_text("Target price", false) {
            Ok(raw) => match raw.parse::<Decimal>() {
                Ok(value) => self.target_price = Some(value),
                Err(_) => println!("Please enter a valid number."),
            },
            Err(error) => println!("Error reading target price: {error}"),
        }
        self
    }

    pub fn thesis(mut self) -> Self {
        let mut io = ConsoleDialogIo::default();
        self = self.thesis_with_io(&mut io);
        self
    }

    pub fn thesis_with_io(mut self, io: &mut dyn DialogIo) -> Self {
        match io.input_text("Trade thesis (optional, max 200 chars)", true) {
            Ok(thesis) if thesis.is_empty() => self.thesis = None,
            Ok(thesis) if thesis.len() > 200 => {
                println!("Thesis must be 200 characters or less");
            }
            Ok(thesis) => self.thesis = Some(thesis),
            Err(error) => println!("Error reading thesis: {error}"),
        }
        self
    }

    pub fn sector(mut self) -> Self {
        let mut io = ConsoleDialogIo::default();
        self = self.sector_with_io(&mut io);
        self
    }

    pub fn sector_with_io(mut self, io: &mut dyn DialogIo) -> Self {
        match io.input_text("Sector (optional, e.g., technology, healthcare)", true) {
            Ok(sector) if sector.is_empty() => self.sector = None,
            Ok(sector) => self.sector = Some(sector),
            Err(error) => println!("Error reading sector: {error}"),
        }
        self
    }

    pub fn asset_class(mut self) -> Self {
        let mut io = ConsoleDialogIo::default();
        self = self.asset_class_with_io(&mut io);
        self
    }

    pub fn asset_class_with_io(mut self, io: &mut dyn DialogIo) -> Self {
        match io.input_text(
            "Asset class (optional, e.g., stocks, options, crypto)",
            true,
        ) {
            Ok(asset_class) if asset_class.is_empty() => self.asset_class = None,
            Ok(asset_class) => self.asset_class = Some(asset_class),
            Err(error) => println!("Error reading asset class: {error}"),
        }
        self
    }

    pub fn context(mut self) -> Self {
        let mut io = ConsoleDialogIo::default();
        self = self.context_with_io(&mut io);
        self
    }

    pub fn context_with_io(mut self, io: &mut dyn DialogIo) -> Self {
        match io.input_text(
            "Trading context (optional, e.g., Elliott Wave, S/R levels)",
            true,
        ) {
            Ok(context) if context.is_empty() => self.context = None,
            Ok(context) => self.context = Some(context),
            Err(error) => println!("Error reading context: {error}"),
        }
        self
    }
}

#[cfg(test)]
mod tests {
    use super::TradeDialogBuilder;
    use crate::dialogs::io::DialogIo;
    use crate::dialogs::io::{scripted_push_input, scripted_push_select, scripted_reset};
    use alpaca_broker::AlpacaBroker;
    use core::TrustFacade;
    use db_sqlite::SqliteDatabase;
    use model::{Currency, Environment, TradeCategory, TradingVehicleCategory};
    use rust_decimal_macros::dec;
    use std::collections::VecDeque;
    use std::io::{Error as IoError, ErrorKind};
    use uuid::Uuid;

    struct ScriptedIo {
        selects: VecDeque<Result<Option<usize>, IoError>>,
        inputs: VecDeque<Result<String, IoError>>,
    }

    impl ScriptedIo {
        fn with(
            selects: Vec<Result<Option<usize>, IoError>>,
            inputs: Vec<Result<String, IoError>>,
        ) -> Self {
            Self {
                selects: selects.into(),
                inputs: inputs.into(),
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

        fn confirm(&mut self, _prompt: &str, _default: bool) -> Result<bool, IoError> {
            Ok(false)
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

    #[test]
    fn new_starts_with_empty_state() {
        let builder = TradeDialogBuilder::new();
        assert!(builder.account.is_none());
        assert!(builder.trading_vehicle.is_none());
        assert!(builder.category.is_none());
        assert!(builder.entry_price.is_none());
        assert!(builder.stop_price.is_none());
        assert!(builder.currency.is_none());
        assert!(builder.quantity.is_none());
        assert!(builder.target_price.is_none());
        assert!(builder.thesis.is_none());
        assert!(builder.sector.is_none());
        assert!(builder.asset_class.is_none());
        assert!(builder.context.is_none());
        assert!(builder.result.is_none());
    }

    #[test]
    fn display_handles_error_result() {
        TradeDialogBuilder {
            account: None,
            trading_vehicle: None,
            category: Some(TradeCategory::Long),
            entry_price: None,
            stop_price: None,
            currency: None,
            quantity: None,
            target_price: None,
            thesis: None,
            sector: None,
            asset_class: None,
            context: None,
            result: Some(Err("synthetic failure".into())),
        }
        .display();
    }

    #[test]
    #[should_panic(expected = "Did you forget to specify trading vehicle")]
    fn build_panics_when_required_inputs_are_missing() {
        let mut trust = test_trust();
        let _ = TradeDialogBuilder::new().build(&mut trust);
    }

    #[test]
    fn build_and_display_successfully_with_valid_inputs() {
        let mut trust = test_trust();
        let account = trust
            .create_account(
                "trade-create",
                "desc",
                Environment::Paper,
                dec!(20),
                dec!(10),
            )
            .expect("account should be created");
        let trading_vehicle = trust
            .create_trading_vehicle("AAPL", None, &TradingVehicleCategory::Stock, "alpaca")
            .expect("trading vehicle should be created");

        let built = TradeDialogBuilder {
            account: Some(account.clone()),
            trading_vehicle: Some(trading_vehicle),
            category: Some(TradeCategory::Long),
            entry_price: Some(dec!(100)),
            stop_price: Some(dec!(95)),
            currency: Some(Currency::USD),
            quantity: Some(1),
            target_price: Some(dec!(110)),
            thesis: Some("breakout".to_string()),
            sector: Some("technology".to_string()),
            asset_class: Some("stocks".to_string()),
            context: Some("range expansion".to_string()),
            result: None,
        }
        .build(&mut trust);

        let trade = built
            .result
            .as_ref()
            .expect("result should be set")
            .as_ref()
            .expect("trade creation should succeed");
        assert_eq!(trade.account_id, account.id);
        assert_eq!(trade.status, model::Status::New);
        assert_eq!(trade.category, TradeCategory::Long);
        assert_eq!(trade.currency, Currency::USD);
        assert_eq!(trade.safety_stop.unit_price, dec!(95));
        assert_eq!(trade.entry.unit_price, dec!(100));
        assert_eq!(trade.target.unit_price, dec!(110));
        assert_eq!(trade.thesis.as_deref(), Some("breakout"));
        assert_eq!(trade.sector.as_deref(), Some("technology"));
        assert_eq!(trade.asset_class.as_deref(), Some("stocks"));
        assert_eq!(trade.context.as_deref(), Some("range expansion"));

        built.display();
    }

    #[test]
    #[should_panic(expected = "called `Option::unwrap()` on a `None` value")]
    fn build_panics_when_account_is_missing_after_trading_vehicle_is_set() {
        let mut trust = test_trust();
        let _ = TradeDialogBuilder {
            account: None,
            trading_vehicle: Some(model::TradingVehicle::default()),
            category: Some(TradeCategory::Long),
            entry_price: Some(dec!(100)),
            stop_price: Some(dec!(95)),
            currency: Some(Currency::USD),
            quantity: Some(1),
            target_price: Some(dec!(110)),
            thesis: None,
            sector: None,
            asset_class: None,
            context: None,
            result: None,
        }
        .build(&mut trust);
    }

    #[test]
    fn io_backed_setters_cover_success_cancel_and_validation_paths() {
        let mut trust = test_trust();
        let account = trust
            .create_account("trade-io", "desc", Environment::Paper, dec!(20), dec!(10))
            .expect("account should be created");
        let _ = trust
            .create_transaction(
                &account,
                &model::TransactionCategory::Deposit,
                dec!(10_000),
                &Currency::USD,
            )
            .expect("deposit should succeed");

        let mut io = ScriptedIo::with(
            vec![Ok(Some(0)), Ok(Some(0)), Ok(None)],
            vec![
                Ok("100".to_string()),
                Ok("95".to_string()),
                Ok("2".to_string()),
                Ok("110".to_string()),
                Ok("breakout".to_string()),
                Ok("technology".to_string()),
                Ok("stocks".to_string()),
                Ok("s/r".to_string()),
                Ok("bad".to_string()),
                Ok("".to_string()),
                Err(IoError::new(ErrorKind::BrokenPipe, "io failed")),
            ],
        );

        let builder = TradeDialogBuilder {
            account: Some(account),
            ..TradeDialogBuilder::new()
        }
        .category_with_io(&mut io)
        .entry_price_with_io(&mut io)
        .stop_price_with_io(&mut io)
        .currency_with_io(&mut trust, &mut io)
        .quantity_with_io(&mut trust, &mut io)
        .target_price_with_io(&mut io)
        .thesis_with_io(&mut io)
        .sector_with_io(&mut io)
        .asset_class_with_io(&mut io)
        .context_with_io(&mut io);

        assert_eq!(builder.category, Some(TradeCategory::Long));
        assert_eq!(builder.entry_price, Some(dec!(100)));
        assert_eq!(builder.stop_price, Some(dec!(95)));
        assert_eq!(builder.currency, Some(Currency::USD));
        assert_eq!(builder.quantity, Some(2));
        assert_eq!(builder.target_price, Some(dec!(110)));
        assert_eq!(builder.thesis.as_deref(), Some("breakout"));
        assert_eq!(builder.sector.as_deref(), Some("technology"));
        assert_eq!(builder.asset_class.as_deref(), Some("stocks"));
        assert_eq!(builder.context.as_deref(), Some("s/r"));

        let unchanged = builder
            .entry_price_with_io(&mut io)
            .thesis_with_io(&mut io)
            .context_with_io(&mut io)
            .category_with_io(&mut io);
        assert_eq!(unchanged.entry_price, Some(dec!(100)));
        assert!(unchanged.thesis.is_none());
        assert_eq!(unchanged.category, Some(TradeCategory::Long));
    }

    #[test]
    fn wrapper_methods_use_default_console_io_in_tests() {
        let mut trust = test_trust();
        let account = trust
            .create_account("trade-wrap", "desc", Environment::Paper, dec!(20), dec!(10))
            .expect("account should be created");
        trust
            .create_transaction(
                &account,
                &model::TransactionCategory::Deposit,
                dec!(10_000),
                &Currency::USD,
            )
            .expect("deposit should succeed");
        let trading_vehicle = trust
            .create_trading_vehicle("AAPL", None, &TradingVehicleCategory::Stock, "alpaca")
            .expect("trading vehicle should be created");

        scripted_reset();
        scripted_push_select(Ok(Some(0)));
        scripted_push_select(Ok(Some(0)));
        scripted_push_select(Ok(Some(0)));
        scripted_push_input(Ok("100".to_string()));
        scripted_push_input(Ok("95".to_string()));
        scripted_push_select(Ok(Some(0)));
        scripted_push_input(Ok("2".to_string()));
        scripted_push_input(Ok("110".to_string()));
        scripted_push_input(Ok("breakout".to_string()));
        scripted_push_input(Ok("technology".to_string()));
        scripted_push_input(Ok("stocks".to_string()));
        scripted_push_input(Ok("s/r".to_string()));

        let builder = TradeDialogBuilder::new()
            .account(&mut trust)
            .trading_vehicle(&mut trust)
            .category()
            .entry_price()
            .stop_price()
            .currency(&mut trust)
            .quantity(&mut trust)
            .target_price()
            .thesis()
            .sector()
            .asset_class()
            .context();

        assert_eq!(
            builder.account.as_ref().expect("selected account").id,
            account.id
        );
        assert_eq!(
            builder
                .trading_vehicle
                .as_ref()
                .expect("selected trading vehicle")
                .id,
            trading_vehicle.id
        );
        assert_eq!(builder.category, Some(TradeCategory::Long));
        assert_eq!(builder.entry_price, Some(dec!(100)));
        assert_eq!(builder.stop_price, Some(dec!(95)));
        assert_eq!(builder.currency, Some(Currency::USD));
        assert_eq!(builder.quantity, Some(2));
        assert_eq!(builder.target_price, Some(dec!(110)));
        assert_eq!(builder.thesis.as_deref(), Some("breakout"));
        assert_eq!(builder.sector.as_deref(), Some("technology"));
        assert_eq!(builder.asset_class.as_deref(), Some("stocks"));
        assert_eq!(builder.context.as_deref(), Some("s/r"));
        scripted_reset();
    }
}
