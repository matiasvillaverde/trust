use crate::views::{AccountOverviewView, TradeOverviewView, TradeView};
use crate::{dialogs::AccountSearchDialog, views::TransactionView};
use dialoguer::{theme::ColorfulTheme, FuzzySelect};
use std::error::Error;
use trust_core::Trust;
use trust_model::{Account, AccountOverview, Trade, TradeOverview, Transaction};

type TradeDialogApproverBuilderResult =
    Option<Result<(Trade, Transaction, AccountOverview, TradeOverview), Box<dyn Error>>>;

pub struct TradeDialogApproverBuilder {
    account: Option<Account>,
    trade: Option<Trade>,
    result: TradeDialogApproverBuilderResult,
}

impl TradeDialogApproverBuilder {
    pub fn new() -> Self {
        TradeDialogApproverBuilder {
            account: None,
            trade: None,
            result: None,
        }
    }

    pub fn build(mut self, trust: &mut Trust) -> TradeDialogApproverBuilder {
        let trade: Trade = self.trade.clone().unwrap();
        self.result = Some(trust.approve(&trade));
        self
    }

    pub fn display(self) {
        match self
            .result
            .expect("No result found, did you forget to call search?")
        {
            Ok((trade, tx, account_overview, trade_overview)) => {
                let account = self.account.clone().unwrap().name;

                println!("Trade approved:");
                TradeView::display_trade(&trade, &self.account.unwrap().name);

                println!("Trade overview:");
                TradeOverviewView::display(trade_overview);

                println!("Transaction moving funds to trade:");
                TransactionView::display(&tx, account.as_str());

                println!("Account overview after funding trade:");
                AccountOverviewView::display(account_overview, account.as_str());
            }
            Err(error) => println!("Error approving trade: {:?}", error),
        }
    }

    pub fn account(mut self, trust: &mut Trust) -> Self {
        let account = AccountSearchDialog::new().search(trust).build();
        match account {
            Ok(account) => self.account = Some(account),
            Err(error) => println!("Error searching account: {:?}", error),
        }
        self
    }

    pub fn search(mut self, trust: &mut Trust) -> Self {
        let trades = trust.search_new_trades(self.account.clone().unwrap().id);
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
