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

use crate::dialogs::{dialog_helpers, AccountSearchDialog, ConsoleDialogIo, DialogIo};
use crate::views::{AccountBalanceView, TradeBalanceView, TradeView, TransactionView};
use core::services::{AdvisoryAlertLevel, TradeProposal};
use core::TrustFacade;
use model::{Account, AccountBalance, Status, Trade, TradeBalance, Transaction};
use std::error::Error;
use std::io::ErrorKind;

type TradeDialogApproverBuilderResult =
    Option<Result<(Trade, Transaction, AccountBalance, TradeBalance), Box<dyn Error>>>;

pub struct FundingDialogBuilder {
    account: Option<Account>,
    trade: Option<Trade>,
    result: TradeDialogApproverBuilderResult,
}

impl FundingDialogBuilder {
    pub fn new() -> Self {
        FundingDialogBuilder {
            account: None,
            trade: None,
            result: None,
        }
    }

    pub fn build(self, trust: &mut TrustFacade) -> FundingDialogBuilder {
        let mut io = ConsoleDialogIo::default();
        self.build_with_io(trust, &mut io)
    }

    fn build_with_io(
        mut self,
        trust: &mut TrustFacade,
        io: &mut dyn DialogIo,
    ) -> FundingDialogBuilder {
        let trade = match dialog_helpers::require(
            self.trade.clone(),
            ErrorKind::InvalidInput,
            "No trade selected for funding",
        ) {
            Ok(trade) => trade,
            Err(error) => {
                self.result = Some(Err(error));
                return self;
            }
        };
        let advisory = trust.advisory_check_trade(TradeProposal {
            account_id: trade.account_id,
            symbol: trade.trading_vehicle.symbol.clone(),
            sector: trade.sector.clone(),
            asset_class: trade.asset_class.clone(),
            entry_price: trade.entry.unit_price,
            quantity: trade.entry.quantity,
        });

        match advisory {
            Ok(result) => {
                match result.level {
                    AdvisoryAlertLevel::Block => {
                        self.result = Some(Err("Trade blocked by advisory limits".into()));
                        return self;
                    }
                    AdvisoryAlertLevel::Warning | AdvisoryAlertLevel::Caution => {
                        println!("Advisory {:?}:", result.level);
                        for warning in result.warnings {
                            println!("  - {warning}");
                        }
                        let proceed = io
                            .confirm("Proceed with funding anyway?", false)
                            .unwrap_or(false);
                        if !proceed {
                            self.result =
                                Some(Err("Funding canceled by user after advisory".into()));
                            return self;
                        }
                    }
                    AdvisoryAlertLevel::Ok => {}
                }
                self.result = Some(trust.fund_trade(&trade));
            }
            Err(error) => {
                self.result = Some(Err(error));
            }
        }
        self
    }

    pub fn display(self) {
        match self
            .result
            .expect("No result found, did you forget to call search?")
        {
            Ok((trade, tx, account_balance, trade_balance)) => {
                let account = self.account.clone().unwrap().name;

                println!("Trade approved:");
                TradeView::display(&trade, &self.account.unwrap().name);

                TradeBalanceView::display(&trade_balance);

                println!("Transaction moving funds to trade:");
                TransactionView::display(&tx, account.as_str());

                println!("Account balance after funding trade:");
                AccountBalanceView::display(account_balance, account.as_str());
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

    pub fn search(self, trust: &mut TrustFacade) -> Self {
        let mut io = ConsoleDialogIo::default();
        self.search_with_io(trust, &mut io)
    }

    fn search_with_io(mut self, trust: &mut TrustFacade, io: &mut dyn DialogIo) -> Self {
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

        let trades = trust.search_trades(account.id, Status::New);
        match trades {
            Ok(trades) => match dialog_helpers::select_from_list(
                io,
                "Trade:",
                &trades,
                "No new trade found for this account",
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

#[cfg(test)]
mod tests;
