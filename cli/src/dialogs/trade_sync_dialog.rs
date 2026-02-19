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
use crate::views::{LogView, OrderView};
use core::TrustFacade;
use model::{Account, BrokerLog, Order, Status, Trade};
use std::error::Error;
use std::io::ErrorKind;

type EntryDialogBuilderResult = Option<Result<(Status, Vec<Order>, BrokerLog), Box<dyn Error>>>;

pub struct SyncTradeDialogBuilder {
    account: Option<Account>,
    trade: Option<Trade>,
    result: EntryDialogBuilderResult,
}

impl SyncTradeDialogBuilder {
    pub fn new() -> Self {
        SyncTradeDialogBuilder {
            account: None,
            trade: None,
            result: None,
        }
    }

    pub fn build(mut self, trust: &mut TrustFacade) -> SyncTradeDialogBuilder {
        let trade = match dialog_helpers::require(
            self.trade.clone(),
            ErrorKind::InvalidInput,
            "No trade selected for sync",
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
            "No account selected for sync",
        ) {
            Ok(account) => account,
            Err(error) => {
                self.result = Some(Err(error));
                return self;
            }
        };
        self.result = Some(trust.sync_trade(&trade, &account));
        self
    }

    pub fn display(self) {
        match self
            .result
            .expect("No result found, did you forget to call search?")
        {
            Ok((status, orders, log)) => {
                if orders.is_empty() {
                    println!(
                        "All orders from trade {} are up to date",
                        self.trade.unwrap().id
                    );
                    return;
                }

                println!("Trade synced, the status is: {status:?}");
                println!();
                println!("Updated orders:");
                OrderView::display_orders(orders);

                println!("Logs:");
                LogView::display(&log);
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
        let mut trades = trust
            .search_trades(account.id, Status::Submitted)
            .unwrap_or_default();
        trades.append(
            &mut trust
                .search_trades(account.id, Status::Filled)
                .unwrap_or_default(),
        );
        trades.append(
            &mut trust
                .search_trades(account.id, Status::Canceled)
                .unwrap_or_default(),
        );

        match dialog_helpers::select_from_list(
            "Trade:",
            &trades,
            "No trade found with status Submitted, Filled or Cancelled",
            "Trade selection was canceled",
        ) {
            Ok(trade) => self.trade = Some(trade),
            Err(error) => self.result = Some(Err(error)),
        }

        self
    }
}
