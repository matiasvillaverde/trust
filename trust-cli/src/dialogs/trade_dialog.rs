use crate::dialogs::{AccountSearchDialog, TradingVehicleSearchDialogBuilder};
use dialoguer::{theme::ColorfulTheme, FuzzySelect, Input};
use rust_decimal::Decimal;
use std::error::Error;
use trust_core::Trust;
use trust_model::{Account, Currency, Trade, TradeCategory, TradingVehicle};

pub struct TradeDialogBuilder {
    account: Option<Account>,
    trading_vehicle: Option<TradingVehicle>,
    category: Option<TradeCategory>,
    entry_price: Option<Decimal>,
    stop_price: Option<Decimal>,
    currency: Option<Currency>,
    quantity: Option<i64>,
    target_price: Option<Decimal>,
    target_order_price: Option<Decimal>,
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
            target_order_price: None,
            result: None,
        }
    }

    pub fn build(self, trust: &mut Trust) -> Self {
        let trading_vehicle_id = self
            .trading_vehicle
            .expect("Did you forget to specify trading vehicle")
            .id;

        let stop = trust.create_stop(
            trading_vehicle_id,
            self.quantity.expect("Did you forget to specify quantity"),
            self.stop_price
                .expect("Did you forget to specify stop price"),
            &self.category.expect("Did you forget to specify category"),
            &self.currency.expect("Did you forget to specify currency"),
        );

        let entry = trust.create_entry(
            trading_vehicle_id,
            self.quantity.expect("Did you forget to specify quantity"),
            self.entry_price
                .expect("Did you forget to specify entry price"),
            &self.category.expect("Did you forget to specify category"),
            &self.currency.expect("Did you forget to specify currency"),
        );

        let target = trust.create_target(
            self.target_price
                .expect("Did you forget to specify target price"),
            &self.currency.expect("Did you forget to specify currency"),
            trading_vehicle_id,
            self.quantity.expect("Did you forget to specify quantity"),
            self.target_order_price
                .expect("Did you forget to specify order price for the target"),
            &self.category.expect("Did you forget to specify category"),
        );

        // TODO: Create Trade
        unimplemented!();
        self
    }

    pub fn display(self) {
        match self
            .result
            .expect("No result found, did you forget to call build?")
        {
            Ok(trade) => println!("Trade created: {:?}", trade),
            Err(error) => println!("Error creating account: {:?}", error),
        }
    }

    pub fn account(mut self, trust: &mut Trust) -> Self {
        let account = AccountSearchDialog::new().search(trust).build();
        match account {
            Ok(account) => self.account = Some(account),
            Err(error) => println!("Error searching account: {:?}", error),
        }
        self
    }

    pub fn trading_vehicle(mut self, trust: &mut Trust) -> Self {
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

    pub fn quantity(mut self, trust: &mut Trust) -> Self {
        let maximum = trust
            .maximum_quantity(
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
                            if parsed > maximum as i64 {
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

    pub fn order_target_price(mut self) -> Self {
        let order_price = Input::new()
            .with_prompt("Order price when target is hit")
            .interact()
            .unwrap(); // TODO: validate that is a valid price
        self.target_order_price = Some(order_price);
        self
    }
}
