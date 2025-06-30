use crate::dialogs::AccountSearchDialog;
use crate::views::{AccountBalanceView, TradeBalanceView, TradeView, TransactionView};
use core::TrustFacade;
use dialoguer::{theme::ColorfulTheme, FuzzySelect};
use model::{Account, AccountBalance, Status, Trade, TradeBalance, Transaction};
use std::error::Error;

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
        let trade: Trade = self
            .trade
            .clone()
            .expect("No trade found, did you forget to select one?");

        match trade.status {
            Status::Funded => {
                self.result = Some(trust.cancel_funded_trade(&trade));
            }
            Status::Submitted => {
                self.result = Some(trust.cancel_submitted_trade(&trade));
            }
            _ => panic!("Trade is not in a cancellable state"),
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
            Err(error) => println!("Error approving trade: {:?}", error),
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

    pub fn search(mut self, trust: &mut TrustFacade) -> Self {
        let funded_trades = trust
            .search_trades(self.account.clone().unwrap().id, Status::Funded)
            .unwrap_or_default();
        let submitted_trades = trust
            .search_trades(self.account.clone().unwrap().id, Status::Submitted)
            .unwrap_or_default();

        let trades = funded_trades
            .into_iter()
            .chain(submitted_trades)
            .collect::<Vec<Trade>>();

        if trades.is_empty() {
            panic!("No trade found, did you forget to fund one?")
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

        self
    }
}
