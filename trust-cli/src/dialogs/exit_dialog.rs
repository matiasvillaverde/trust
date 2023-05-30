use crate::dialogs::AccountSearchDialog;
use crate::views::{AccountOverviewView, TradeOverviewView, TradeView, TransactionView};
use dialoguer::{theme::ColorfulTheme, FuzzySelect};
use std::error::Error;
use trust_core::Trust;
use trust_model::{Account, AccountOverview, Trade, TradeOverview, Transaction};

type ExitDialogBuilderResult =
    Option<Result<(Transaction, Transaction, TradeOverview, AccountOverview), Box<dyn Error>>>;

pub struct ExitDialogBuilder {
    account: Option<Account>,
    trade: Option<Trade>,
    result: ExitDialogBuilderResult,
}

impl ExitDialogBuilder {
    pub fn new() -> Self {
        ExitDialogBuilder {
            account: None,
            trade: None,
            result: None,
        }
    }

    pub fn record_stop(mut self, trust: &mut Trust) -> ExitDialogBuilder {
        let trade: Trade = self.trade.clone().unwrap();
        self.result = Some(trust.record_stop(&trade));
        self
    }

    pub fn record_target(mut self, trust: &mut Trust) -> ExitDialogBuilder {
        let trade: Trade = self.trade.clone().unwrap();
        self.result = Some(trust.record_target(&trade));
        self
    }

    pub fn display(self) {
        match self
            .result
            .expect("No result found, did you forget to call search?")
        {
            Ok((tx_exit, tx_payment, trade_overview, account_overview)) => {
                let account_name = self.account.clone().unwrap().name;

                println!("Trade exit executed:");
                TradeView::display_trade(&self.trade.unwrap(), account_name.as_str());

                println!("With transaction of exit:");
                TransactionView::display(&tx_exit, account_name.as_str());

                println!("With transaction of payment back to the account:");
                TransactionView::display(&tx_payment, account_name.as_str());

                println!("Trade overview:");
                TradeOverviewView::display(trade_overview);

                println!("Account overview:");
                AccountOverviewView::display(account_overview, account_name.as_str());
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
        let trades = trust.search_all_trades_in_market(self.account.clone().unwrap().id);
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
