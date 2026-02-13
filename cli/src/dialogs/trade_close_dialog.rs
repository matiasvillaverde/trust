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
use crate::views::{LogView, TradeBalanceView, TradeView};
use core::TrustFacade;
use dialoguer::{theme::ColorfulTheme, FuzzySelect};
use model::{Account, BrokerLog, DistributionResult, Status, Trade, TradeBalance};
use std::error::Error;

type CancelDialogBuilderResult =
    Option<Result<(TradeBalance, BrokerLog, Option<DistributionResult>), Box<dyn Error>>>;

pub struct CloseDialogBuilder {
    account: Option<Account>,
    trade: Option<Trade>,
    auto_distribute: bool,
    result: CancelDialogBuilderResult,
}

impl CloseDialogBuilder {
    pub fn new() -> Self {
        CloseDialogBuilder {
            account: None,
            trade: None,
            auto_distribute: false,
            result: None,
        }
    }

    pub fn build(mut self, trust: &mut TrustFacade) -> CloseDialogBuilder {
        let trade: Trade = self
            .trade
            .clone()
            .expect("No trade found, did you forget to select one?");

        self.result = if self.auto_distribute {
            Some(trust.close_trade_with_auto_distribution(&trade))
        } else {
            Some(
                trust
                    .close_trade(&trade)
                    .map(|(balance, log)| (balance, log, None)),
            )
        };
        self
    }

    pub fn display(self) {
        match self
            .result
            .expect("No result found, did you forget to call search?")
        {
            Ok((trade_balance, log, distribution_result)) => {
                let account_name = self.account.clone().unwrap().name;

                println!("Trade close executed:");
                TradeView::display(&self.trade.unwrap(), account_name.as_str());
                TradeBalanceView::display(&trade_balance);
                LogView::display(&log);
                if let Some(distribution) = distribution_result {
                    println!(
                        "Auto distribution executed: {} transfer records",
                        distribution.transactions_created.len()
                    );
                }
            }
            Err(error) => println!("Error approving trade: {error:?}"),
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
                    panic!("No trade found, did you forget to create one?")
                }
                let trade = FuzzySelect::with_theme(&ColorfulTheme::default())
                    .with_prompt("Trade:")
                    .items(&trades[..])
                    .default(0)
                    .interact_opt()
                    .unwrap()
                    .map(|index| trades.get(index).unwrap())
                    .unwrap();

                self.trade = Some(trade.to_owned());
            }
            Err(error) => self.result = Some(Err(error)),
        }

        self
    }

    pub fn auto_distribute(mut self, auto_distribute: bool) -> Self {
        self.auto_distribute = auto_distribute;
        self
    }
}
