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
use crate::views::{LogView, TradeBalanceView, TradeView};
use core::TrustFacade;
use model::{Account, BrokerLog, Status, Trade};
use std::error::Error;
use std::io::ErrorKind;

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
        let trade = match dialog_helpers::require(
            self.trade.clone(),
            ErrorKind::InvalidInput,
            "No trade selected for submission",
        ) {
            Ok(trade) => trade,
            Err(error) => {
                self.result = Some(Err(error));
                return self;
            }
        };

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
                let account_name = self
                    .account
                    .as_ref()
                    .map_or("<unknown account>", |account| account.name.as_str());
                TradeView::display(&trade, account_name);

                TradeBalanceView::display(&trade.balance);

                println!("Stop:");
                crate::views::OrderView::display(trade.safety_stop);

                println!("Entry:");
                crate::views::OrderView::display(trade.entry);

                println!("Target:");
                crate::views::OrderView::display(trade.target);

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
        let account = match dialog_helpers::require(
            self.account.clone(),
            ErrorKind::InvalidInput,
            "No account selected",
        ) {
            Ok(account) => account,
            Err(error) => {
                self.result = Some(Err(error));
                return self;
            }
        };

        let trades = trust.search_trades(account.id, Status::Funded);
        match trades {
            Ok(trades) => match dialog_helpers::select_from_list(
                "Trade:",
                &trades,
                "No funded trade found for this account",
                "Trade selection was canceled",
            ) {
                Ok(trade) => self.trade = Some(trade),
                Err(error) => self.result = Some(Err(error)),
            },
            Err(error) => self.result = Some(Err(error)),
        }

        self
    }
}
