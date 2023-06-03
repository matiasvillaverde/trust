use crate::{
    dialogs::{AccountSearchDialog, TradingVehicleSearchDialogBuilder},
    views::TradeOverviewView,
    views::TradeView,
};
use dialoguer::{theme::ColorfulTheme, FuzzySelect, Input};
use rust_decimal::Decimal;
use std::error::Error;
use trust_core::TrustFacade;
use trust_model::{Account, Currency, DraftTrade, Trade, TradeCategory, TradingVehicle};

pub struct TradeDialogBuilder {
    account: Option<Account>,
    trading_vehicle: Option<TradingVehicle>,
    category: Option<TradeCategory>,
    entry_price: Option<Decimal>,
    stop_price: Option<Decimal>,
    currency: Option<Currency>,
    quantity: Option<i64>,
    target_price: Option<Decimal>,
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
                TradeView::display_trade(&trade, &self.account.unwrap().name);
                TradeOverviewView::display(trade.overview);
            }
            Err(error) => println!("Error creating account: {:?}", error),
        }
    }

    pub fn account(mut self, trust: &mut TrustFacade) -> Self {
        let account = AccountSearchDialog::new().search(trust).build();
        match account {
            Ok(account) => self.account = Some(account),
            Err(error) => println!("Error searching account: {:?}", error),
        }
        self
    }

    pub fn trading_vehicle(mut self, trust: &mut TrustFacade) -> Self {
        let tv = TradingVehicleSearchDialogBuilder::new()
            .search(trust)
            .build();
        match tv {
            Ok(tv) => self.trading_vehicle = Some(tv),
            Err(error) => println!("Error searching trading vehicle: {:?}", error),
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

    pub fn currency(mut self) -> Self {
        let currencies = Currency::all(); // TODO: Show only currencies available

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
        let maximum = trust
            .calculate_maximum_quantity(
                self.account.clone().unwrap().id,
                self.entry_price.unwrap(),
                self.stop_price.unwrap(),
                &self.currency.unwrap(),
            )
            .unwrap_or_else(|error| {
                println!("Error calculating maximum quantity {}", error);
                0
            });

        println!("Maximum quantity: {}", maximum);

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
}
