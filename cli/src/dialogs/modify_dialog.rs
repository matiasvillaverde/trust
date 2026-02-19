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

use crate::dialogs::{dialog_helpers, AccountSearchDialog};
use crate::views::{OrderView, TradeBalanceView, TradeView};
use core::TrustFacade;
use dialoguer::Input;
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
                let account_name = self.account.as_ref().map_or("<unknown account>", |account| {
                    account.name.as_str()
                });
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
        let account = match dialog_helpers::require(self.account.clone(), ErrorKind::InvalidInput, "No account selected")
        {
            Ok(account) => account,
            Err(error) => {
                self.result = Some(Err(error));
                return self;
            }
        };

        let trades = trust.search_trades(account.id, Status::Filled);
        match trades {
            Ok(trades) => match dialog_helpers::select_from_list(
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
        let stop_price = match Input::new().with_prompt("New price").interact() {
            Ok(stop_price) => stop_price,
            Err(error) => {
                println!("Error reading new price: {error:?}");
                return self;
            }
        };
        self.new_price = Some(stop_price);
        self
    }
}
