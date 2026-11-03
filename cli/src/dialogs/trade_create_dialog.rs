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
use model::{Account, Currency, DraftTrade, OrderCategory, Trade, TradeCategory, TradingVehicle};
use rust_decimal::Decimal;
use std::error::Error;

pub struct TradeDialogBuilder {
    account: Option<Account>,
    trading_vehicle: Option<TradingVehicle>,
    category: Option<TradeCategory>,
    entry_price: Option<Decimal>,
    stop_price: Option<Decimal>,
    safety_order_category: Option<OrderCategory>,
    currency: Option<Currency>,
    quantity: Option<Decimal>,
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
            safety_order_category: None,
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
        self.result = Some((|| {
            let draft = DraftTrade {
                trading_vehicle: crate::dialogs::dialog_helpers::require(
                    self.trading_vehicle.clone(),
                    std::io::ErrorKind::InvalidInput,
                    "Select a trading vehicle",
                )?,
                account: crate::dialogs::dialog_helpers::require(
                    self.account.clone(),
                    std::io::ErrorKind::InvalidInput,
                    "Select an account",
                )?,
                quantity: crate::dialogs::dialog_helpers::require(
                    self.quantity,
                    std::io::ErrorKind::InvalidInput,
                    "Enter a quantity",
                )?,
                currency: crate::dialogs::dialog_helpers::require(
                    self.currency,
                    std::io::ErrorKind::InvalidInput,
                    "Select a currency",
                )?,
                category: crate::dialogs::dialog_helpers::require(
                    self.category,
                    std::io::ErrorKind::InvalidInput,
                    "Select a trade direction",
                )?,
                thesis: self.thesis.clone(),
                sector: self.sector.clone(),
                asset_class: self.asset_class.clone(),
                context: self.context.clone(),
            };
            trust.create_trade_with_safety_order_category(
                draft,
                crate::dialogs::dialog_helpers::require(
                    self.stop_price,
                    std::io::ErrorKind::InvalidInput,
                    "Enter a stop price",
                )?,
                crate::dialogs::dialog_helpers::require(
                    self.entry_price,
                    std::io::ErrorKind::InvalidInput,
                    "Enter an entry price",
                )?,
                crate::dialogs::dialog_helpers::require(
                    self.target_price,
                    std::io::ErrorKind::InvalidInput,
                    "Enter a target price",
                )?,
                self.safety_order_category.unwrap_or(OrderCategory::Stop),
            )
        })());
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
            Err(error) => println!("Error creating trade: {error:?}"),
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

    pub fn safety_order_category(mut self) -> Self {
        let mut io = ConsoleDialogIo::default();
        self = self.safety_order_category_with_io(&mut io);
        self
    }

    pub fn safety_order_category_with_io(mut self, io: &mut dyn DialogIo) -> Self {
        let available_categories = [OrderCategory::Stop, OrderCategory::StopLimit];
        let labels: Vec<String> = available_categories
            .iter()
            .map(ToString::to_string)
            .collect();
        match io.select_index("Safety order type:", &labels, 0) {
            Ok(Some(index)) => {
                self.safety_order_category = available_categories.get(index).copied();
            }
            Ok(None) => {}
            Err(error) => println!("Error selecting safety order type: {error}"),
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
                (
                    rust_decimal::Decimal::ZERO,
                    rust_decimal::Decimal::ONE,
                    rust_decimal::Decimal::ZERO,
                )
            }
        };

        println!("Base quantity (rules only): {base_quantity}");
        println!("Level multiplier: {level_multiplier}x");
        println!("Maximum quantity (level-adjusted): {maximum}");

        match io.input_text("Quantity", false) {
            Ok(raw) => match raw.parse::<Decimal>() {
                Ok(parsed) if parsed > maximum => {
                    println!("Please enter a number below your maximum allowed");
                }
                Ok(parsed) if parsed <= Decimal::ZERO => println!("Please enter a number above 0"),
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
mod tests;
