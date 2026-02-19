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
use crate::views::{AccountBalanceView, TradeBalanceView, TradeView, TransactionView};
use core::TrustFacade;
use model::{Account, AccountBalance, Status, Trade, TradeBalance, Transaction};
use std::error::Error;
use std::io::ErrorKind;

type CancelDialogBuilderResult =
    Option<Result<(TradeBalance, AccountBalance, Transaction), Box<dyn Error>>>;

pub struct CancelDialogBuilder {
    account: Option<Account>,
    trade: Option<Trade>,
    result: CancelDialogBuilderResult,
}

impl CancelDialogBuilder {
    pub fn new() -> Self {
        CancelDialogBuilder {
            account: None,
            trade: None,
            result: None,
        }
    }

    pub fn build(mut self, trust: &mut TrustFacade) -> CancelDialogBuilder {
        let trade: Trade = match dialog_helpers::require(
            self.trade.clone(),
            ErrorKind::InvalidInput,
            "No trade selected for cancellation",
        ) {
            Ok(trade) => trade,
            Err(error) => {
                self.result = Some(Err(error));
                return self;
            }
        };

        match trade.status {
            Status::Funded => {
                self.result = Some(trust.cancel_funded_trade(&trade));
            }
            Status::Submitted => {
                self.result = Some(trust.cancel_submitted_trade(&trade));
            }
            _ => {
                self.result = Some(Err(Box::new(std::io::Error::new(
                    ErrorKind::InvalidInput,
                    "Trade is not in a cancellable state",
                ))));
            }
        }

        self
    }

    pub fn display(self) {
        match self
            .result
            .expect("No result found, did you forget to call search?")
        {
            Ok((trade_balance, account_o, tx)) => {
                let account_name = self.account.clone().unwrap().name;

                println!("Trade cancel executed:");
                TradeView::display(&self.trade.unwrap(), account_name.as_str());
                TradeBalanceView::display(&trade_balance);
                AccountBalanceView::display(account_o, account_name.as_str());
                TransactionView::display(&tx, account_name.as_str());
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

        let funded_trades = trust
            .search_trades(account.id, Status::Funded)
            .unwrap_or_default();
        let submitted_trades = trust
            .search_trades(account.id, Status::Submitted)
            .unwrap_or_default();

        let trades = funded_trades
            .into_iter()
            .chain(submitted_trades)
            .collect::<Vec<Trade>>();

        match dialog_helpers::select_from_list(
            "Trade:",
            &trades,
            "No funded or submitted trades found for this account",
            "Trade selection was canceled",
        ) {
            Ok(trade) => self.trade = Some(trade),
            Err(error) => self.result = Some(Err(error)),
        }

        self
    }
}
