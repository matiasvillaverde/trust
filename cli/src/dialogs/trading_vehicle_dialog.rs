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

use std::error::Error;

use crate::dialogs::{dialog_helpers, ConsoleDialogIo, DialogIo};
use core::TrustFacade;
use model::{TradingVehicle, TradingVehicleCategory};

use crate::views::TradingVehicleView;

pub struct TradingVehicleDialogBuilder {
    symbol: Option<String>,
    isin: Option<String>,
    category: Option<TradingVehicleCategory>,
    broker: Option<String>,
    result: Option<Result<TradingVehicle, Box<dyn Error>>>,
}

impl TradingVehicleDialogBuilder {
    pub fn new() -> Self {
        TradingVehicleDialogBuilder {
            symbol: None,
            isin: None,
            category: None,
            broker: None,
            result: None,
        }
    }

    pub fn build(mut self, trust: &mut TrustFacade) -> TradingVehicleDialogBuilder {
        let symbol = self.symbol.clone().expect("Select symbol first");
        let category = self.category.expect("Select category first");
        let broker = self.broker.clone().expect("Select broker first");

        let isin = self.isin.clone().and_then(|value| {
            if value.trim().is_empty() {
                None
            } else {
                Some(value)
            }
        });

        self.result =
            Some(trust.create_trading_vehicle(&symbol, isin.as_deref(), &category, &broker));
        self
    }

    pub fn display(self) {
        match self
            .result
            .expect("No result found, did you forget to call build?")
        {
            Ok(tv) => TradingVehicleView::display(tv),
            Err(error) => println!("Error creating trading vehicle: {error:?}"),
        }
    }

    pub fn category(self) -> Self {
        let mut io = ConsoleDialogIo::default();
        self.category_with_io(&mut io)
    }

    pub fn category_with_io(mut self, io: &mut dyn DialogIo) -> Self {
        let available_categories = TradingVehicleCategory::all();
        let labels = available_categories
            .iter()
            .map(std::string::ToString::to_string)
            .collect::<Vec<_>>();
        if let Ok(Some(index)) = io.select_index("Category:", &labels, 0) {
            self.category = available_categories.get(index).copied();
        }
        self
    }

    pub fn symbol(self) -> Self {
        let mut io = ConsoleDialogIo::default();
        self.symbol_with_io(&mut io)
    }

    pub fn symbol_with_io(mut self, io: &mut dyn DialogIo) -> Self {
        if let Ok(value) = io.input_text("Symbol: ", false) {
            self.symbol = Some(value);
        }
        self
    }

    pub fn isin(self) -> Self {
        let mut io = ConsoleDialogIo::default();
        self.isin_with_io(&mut io)
    }

    pub fn isin_with_io(mut self, io: &mut dyn DialogIo) -> Self {
        if let Ok(value) = io.input_text("ISIN (optional): ", true) {
            self.isin = Some(value);
        }
        self
    }

    pub fn broker(self) -> Self {
        let mut io = ConsoleDialogIo::default();
        self.broker_with_io(&mut io)
    }

    pub fn broker_with_io(mut self, io: &mut dyn DialogIo) -> Self {
        if let Ok(value) = io.input_text("Broker: ", false) {
            self.broker = Some(value);
        }
        self
    }
}

#[cfg(test)]
mod tests {
    use super::{TradingVehicleDialogBuilder, TradingVehicleSearchDialogBuilder};
    use crate::dialogs::DialogIo;
    use alpaca_broker::AlpacaBroker;
    use core::TrustFacade;
    use db_sqlite::SqliteDatabase;
    use model::TradingVehicleCategory;
    use std::collections::VecDeque;
    use std::io::Error as IoError;
    use uuid::Uuid;

    #[derive(Default)]
    struct ScriptedIo {
        selections: VecDeque<Result<Option<usize>, IoError>>,
        text_inputs: VecDeque<Result<String, IoError>>,
    }

    impl DialogIo for ScriptedIo {
        fn select_index(
            &mut self,
            _prompt: &str,
            _labels: &[String],
            _default: usize,
        ) -> Result<Option<usize>, IoError> {
            self.selections.pop_front().unwrap_or(Ok(None))
        }

        fn confirm(&mut self, _prompt: &str, _default: bool) -> Result<bool, IoError> {
            Ok(false)
        }

        fn input_text(&mut self, _prompt: &str, _allow_empty: bool) -> Result<String, IoError> {
            self.text_inputs
                .pop_front()
                .unwrap_or_else(|| Ok(String::new()))
        }
    }

    #[test]
    fn category_with_io_selects_expected_category() {
        let mut io = ScriptedIo::default();
        io.selections.push_back(Ok(Some(0)));
        let dialog = TradingVehicleDialogBuilder::new().category_with_io(&mut io);
        assert_eq!(dialog.category, Some(TradingVehicleCategory::Crypto));
    }

    #[test]
    fn category_with_io_keeps_none_when_cancelled() {
        let mut io = ScriptedIo::default();
        io.selections.push_back(Ok(None));
        let dialog = TradingVehicleDialogBuilder::new().category_with_io(&mut io);
        assert_eq!(dialog.category, None);
    }

    #[test]
    fn text_input_setters_with_io_set_values_and_preserve_state_on_error() {
        let mut success_io = ScriptedIo::default();
        success_io.text_inputs.push_back(Ok("AAPL".to_string()));
        success_io
            .text_inputs
            .push_back(Ok("US0378331005".to_string()));
        success_io.text_inputs.push_back(Ok("alpaca".to_string()));
        let dialog = TradingVehicleDialogBuilder::new()
            .symbol_with_io(&mut success_io)
            .isin_with_io(&mut success_io)
            .broker_with_io(&mut success_io);
        assert_eq!(dialog.symbol.as_deref(), Some("AAPL"));
        assert_eq!(dialog.isin.as_deref(), Some("US0378331005"));
        assert_eq!(dialog.broker.as_deref(), Some("alpaca"));

        let mut error_io = ScriptedIo::default();
        error_io
            .text_inputs
            .push_back(Err(IoError::other("input failed")));
        let dialog = TradingVehicleDialogBuilder::new().symbol_with_io(&mut error_io);
        assert_eq!(dialog.symbol, None);
    }

    fn test_trust() -> TrustFacade {
        let path = std::env::temp_dir().join(format!("trust-test-tv-{}.db", Uuid::new_v4()));
        let db = SqliteDatabase::new(path.to_str().expect("valid temp path"));
        TrustFacade::new(Box::new(db), Box::<AlpacaBroker>::default())
    }

    #[test]
    fn build_creates_trading_vehicle_and_normalizes_empty_isin() {
        let mut trust = test_trust();
        let builder = TradingVehicleDialogBuilder {
            symbol: Some("AAPL".to_string()),
            isin: Some("".to_string()),
            category: Some(TradingVehicleCategory::Stock),
            broker: Some("alpaca".to_string()),
            result: None,
        };

        let built = builder.build(&mut trust);
        let tv = built
            .result
            .expect("result should exist")
            .expect("creation should succeed");
        assert_eq!(tv.symbol, "AAPL");
        assert_eq!(tv.isin.as_deref(), Some("ALPACA:AAPL"));
    }

    #[test]
    fn search_with_io_selects_and_handles_cancel_and_empty_list() {
        let mut trust = test_trust();
        let created = trust
            .create_trading_vehicle("MSFT", None, &TradingVehicleCategory::Stock, "alpaca")
            .expect("vehicle should be created");

        let mut selected_io = ScriptedIo::default();
        selected_io.selections.push_back(Ok(Some(0)));
        let selected =
            TradingVehicleSearchDialogBuilder::new().search_with_io(&mut trust, &mut selected_io);
        let selected_tv = selected.build().expect("selected vehicle should exist");
        assert_eq!(selected_tv.id, created.id);

        let mut canceled_io = ScriptedIo::default();
        canceled_io.selections.push_back(Ok(None));
        let canceled =
            TradingVehicleSearchDialogBuilder::new().search_with_io(&mut trust, &mut canceled_io);
        let canceled_error = canceled.build().expect_err("cancel should produce error");
        assert!(canceled_error.to_string().contains("canceled"));

        let mut empty_trust = test_trust();
        let mut empty_io = ScriptedIo::default();
        let empty = TradingVehicleSearchDialogBuilder::new()
            .search_with_io(&mut empty_trust, &mut empty_io);
        let empty_error = empty
            .build()
            .expect_err("empty list should produce an error");
        assert!(empty_error
            .to_string()
            .contains("No trading vehicles found"));
    }

    #[test]
    fn display_handles_success_and_error_result_paths() {
        let mut trust = test_trust();
        let builder = TradingVehicleDialogBuilder {
            symbol: Some("NVDA".to_string()),
            isin: None,
            category: Some(TradingVehicleCategory::Stock),
            broker: Some("alpaca".to_string()),
            result: None,
        };
        builder.build(&mut trust).display();

        TradingVehicleDialogBuilder {
            symbol: None,
            isin: None,
            category: None,
            broker: None,
            result: Some(Err("forced error".into())),
        }
        .display();
    }

    #[test]
    fn search_builder_display_and_search_error_path_do_not_panic() {
        let mut trust = test_trust();
        let mut io = ScriptedIo::default();
        io.selections
            .push_back(Err(IoError::other("selection failed")));
        TradingVehicleSearchDialogBuilder::new()
            .search_with_io(&mut trust, &mut io)
            .display();
    }
}

pub struct TradingVehicleSearchDialogBuilder {
    result: Option<Result<TradingVehicle, Box<dyn Error>>>,
}

impl TradingVehicleSearchDialogBuilder {
    pub fn new() -> Self {
        TradingVehicleSearchDialogBuilder { result: None }
    }

    pub fn build(self) -> Result<TradingVehicle, Box<dyn Error>> {
        self.result
            .expect("No result found, did you forget to call search?")
    }

    pub fn display(self) {
        match self
            .result
            .expect("No result found, did you forget to call search?")
        {
            Ok(tv) => {
                TradingVehicleView::display(tv);
            }
            Err(error) => println!("Error searching Trading Vehicles: {error:?}"),
        }
    }

    pub fn search(mut self, trust: &mut TrustFacade) -> Self {
        let mut io = ConsoleDialogIo::default();
        self = self.search_with_io(trust, &mut io);
        self
    }

    pub fn search_with_io(mut self, trust: &mut TrustFacade, io: &mut dyn DialogIo) -> Self {
        let trading_vehicles = trust.search_trading_vehicles();
        match trading_vehicles {
            Ok(tvs) => {
                match dialog_helpers::select_from_list(
                    io,
                    "Trading Vehicle:",
                    &tvs,
                    "No trading vehicles found, did you forget to add one?",
                    "Trading vehicle selection was canceled",
                ) {
                    Ok(tv) => self.result = Some(Ok(tv)),
                    Err(error) => self.result = Some(Err(error)),
                }
            }
            Err(error) => self.result = Some(Err(error)),
        }

        self
    }
}
