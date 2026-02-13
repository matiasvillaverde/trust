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
    dialogs::{AccountSearchDialog, TradingVehicleSearchDialogBuilder},
    views::TradeBalanceView,
    views::TradeView,
};
use core::TrustFacade;
use dialoguer::{theme::ColorfulTheme, FuzzySelect, Input};
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
        let available_categories = TradeCategory::all();

        let selected_category = FuzzySelect::with_theme(&ColorfulTheme::default())
            .with_prompt("Category:")
            .items(&available_categories[..])
            .interact()
            .map(|index| available_categories.get(index).unwrap())
            .unwrap();

        self.category = Some(*selected_category);
        self
    }

    pub fn entry_price(mut self) -> Self {
        let entry_price = Input::new().with_prompt("Entry price").interact().unwrap();

        self.entry_price = Some(entry_price);
        self
    }

    pub fn stop_price(mut self) -> Self {
        let stop_price = Input::new().with_prompt("Stop price").interact().unwrap();

        self.stop_price = Some(stop_price);
        self
    }

    pub fn currency(mut self, trust: &mut TrustFacade) -> Self {
        let currencies: Vec<Currency> = trust
            .search_all_balances(self.account.clone().unwrap().id)
            .unwrap()
            .into_iter()
            .map(|balance| balance.currency)
            .collect();

        let selected_currency = FuzzySelect::with_theme(&ColorfulTheme::default())
            .with_prompt("Currency:")
            .items(&currencies[..])
            .interact()
            .map(|index| currencies.get(index).unwrap())
            .unwrap();

        self.currency = Some(*selected_currency);
        self
    }

    pub fn quantity(mut self, trust: &mut TrustFacade) -> Self {
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

        let quantity = Input::new()
            .with_prompt("Quantity")
            .validate_with({
                |input: &String| -> Result<(), &str> {
                    match input.parse::<i64>() {
                        Ok(parsed) => {
                            if parsed > maximum {
                                return Err("Please enter a number below your maximum allowed");
                            } else if parsed == 0 {
                                return Err("Please enter a number above 0");
                            }
                            Ok(())
                        }
                        Err(_) => Err("Please enter a valid number."),
                    }
                }
            })
            .interact()
            .unwrap()
            .parse::<i64>()
            .unwrap();

        self.quantity = Some(quantity);
        self
    }

    pub fn target_price(mut self) -> Self {
        let target_price = Input::new().with_prompt("Target price").interact().unwrap();
        self.target_price = Some(target_price);
        self
    }

    pub fn thesis(mut self) -> Self {
        let thesis: String = Input::new()
            .with_prompt("Trade thesis (optional, max 200 chars)")
            .allow_empty(true)
            .validate_with(|input: &String| -> Result<(), &str> {
                if input.len() > 200 {
                    Err("Thesis must be 200 characters or less")
                } else {
                    Ok(())
                }
            })
            .interact()
            .unwrap();

        self.thesis = if thesis.is_empty() {
            None
        } else {
            Some(thesis)
        };
        self
    }

    pub fn sector(mut self) -> Self {
        let sector: String = Input::new()
            .with_prompt("Sector (optional, e.g., technology, healthcare)")
            .allow_empty(true)
            .interact()
            .unwrap();

        self.sector = if sector.is_empty() {
            None
        } else {
            Some(sector)
        };
        self
    }

    pub fn asset_class(mut self) -> Self {
        let asset_class: String = Input::new()
            .with_prompt("Asset class (optional, e.g., stocks, options, crypto)")
            .allow_empty(true)
            .interact()
            .unwrap();

        self.asset_class = if asset_class.is_empty() {
            None
        } else {
            Some(asset_class)
        };
        self
    }

    pub fn context(mut self) -> Self {
        let context: String = Input::new()
            .with_prompt("Trading context (optional, e.g., Elliott Wave, S/R levels)")
            .allow_empty(true)
            .interact()
            .unwrap();

        self.context = if context.is_empty() {
            None
        } else {
            Some(context)
        };
        self
    }
}
