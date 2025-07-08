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

use crate::dialogs::AccountSearchDialog;
use crate::views::{OrderView, TradeBalanceView, TradeView};
use core::TrustFacade;
use dialoguer::{theme::ColorfulTheme, FuzzySelect, Input};
use model::{Account, Status, Trade};
use rust_decimal::Decimal;
use std::error::Error;

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
        let trade = self
            .trade
            .clone()
            .expect("No trade found, did you forget to call search?");

        let account = self
            .account
            .clone()
            .expect("No account found, did you forget to call account?");
        let stop_price = self
            .new_price
            .expect("No stop price found, did you forget to call stop_price?");

        match trust.modify_stop(&trade, &account, stop_price) {
            Ok(trade) => self.result = Some(Ok(trade)),
            Err(error) => self.result = Some(Err(error)),
        }
        self
    }

    pub fn build_target(mut self, trust: &mut TrustFacade) -> ModifyDialogBuilder {
        let trade = self
            .trade
            .clone()
            .expect("No trade found, did you forget to call search?");

        let account = self
            .account
            .clone()
            .expect("No account found, did you forget to call account?");
        let target_price = self
            .new_price
            .expect("No target price found, did you forget to call stop_price?");

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
                TradeView::display(&trade, &self.account.unwrap().name);

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
        let trades = trust.search_trades(self.account.clone().unwrap().id, Status::Filled);
        match trades {
            Ok(trades) => {
                if trades.is_empty() {
                    panic!("No trade found with the status filled, did you forget to submit one?")
                }
                let trade = FuzzySelect::with_theme(&ColorfulTheme::default())
                    .with_prompt("Trade:")
                    .items(&trades[..])
                    .default(0)
                    .interact_opt()
                    .unwrap()
                    .map(|index| trades.get(index).unwrap())
                    .unwrap();

                println!("Trade selected:");
                TradeView::display(trade, &self.account.clone().unwrap().name);
                self.trade = Some(trade.to_owned());
            }
            Err(error) => self.result = Some(Err(error)),
        }

        self
    }

    pub fn new_price(mut self) -> Self {
        let stop_price = Input::new().with_prompt("New price").interact().unwrap();
        self.new_price = Some(stop_price);
        self
    }
}
