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
use crate::views::{LogView, OrderView, TradeBalanceView, TradeView};
use core::TrustFacade;
use dialoguer::{theme::ColorfulTheme, FuzzySelect};
use model::{Account, BrokerLog, Status, Trade};
use std::error::Error;

type TradeDialogApproverBuilderResult = Option<Result<(Trade, BrokerLog), Box<dyn Error>>>;

pub struct SubmitDialogBuilder {
    account: Option<Account>,
    trade: Option<Trade>,
    result: TradeDialogApproverBuilderResult,
}

impl SubmitDialogBuilder {
    pub fn new() -> Self {
        SubmitDialogBuilder {
            account: None,
            trade: None,
            result: None,
        }
    }

    pub fn build(mut self, trust: &mut TrustFacade) -> SubmitDialogBuilder {
        let trade: Trade = self.trade.clone().unwrap();
        self.result = Some(trust.submit_trade(&trade));
        self
    }

    pub fn display(self) {
        match self
            .result
            .expect("No result found, did you forget to call search?")
        {
            Ok((trade, log)) => {
                println!("Trade submitted:");
                TradeView::display(&trade, &self.account.unwrap().name);

                TradeBalanceView::display(&trade.balance);

                println!("Stop:");
                OrderView::display(trade.safety_stop);

                println!("Entry:");
                OrderView::display(trade.entry);

                println!("Target:");
                OrderView::display(trade.target);

                LogView::display(&log);
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
        let trades = trust.search_trades(self.account.clone().unwrap().id, Status::Funded);
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
}
