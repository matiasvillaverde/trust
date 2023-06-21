use crate::dialogs::AccountSearchDialog;
use crate::views::{AccountOverviewView, TradeOverviewView, TradeView, TransactionView};
use dialoguer::{theme::ColorfulTheme, FuzzySelect};
use std::error::Error;
use trust_core::TrustFacade;
use trust_model::{Account, AccountOverview, Status, Trade, TradeOverview, Transaction};

type CancelDialogBuilderResult =
    Option<Result<(TradeOverview, AccountOverview, Transaction), Box<dyn Error>>>;

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

        self.result = Some(trust.cancel_funded_trade(&trade));
        self
    }

    pub fn display(self) {
        match self
            .result
            .expect("No result found, did you forget to call search?")
        {
            Ok((trade_overview, account_o, tx)) => {
                let account_name = self.account.clone().unwrap().name;

                println!("Trade cancel executed:");
                TradeView::display(&self.trade.unwrap(), account_name.as_str());
                TradeOverviewView::display(&trade_overview);
                AccountOverviewView::display(account_o, account_name.as_str());
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
        let trades = trust.search_trades(self.account.clone().unwrap().id, Status::Funded);
        match trades {
            Ok(trades) => {
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
            }
            Err(error) => self.result = Some(Err(error)),
        }

        self
    }
}
