use crate::dialogs::AccountSearchDialog;
use crate::views::{TradeOverviewView, TradeView};
use dialoguer::{theme::ColorfulTheme, FuzzySelect};
use std::error::Error;
use trust_core::Trust;
use trust_model::{Account, Trade};

type EntryDialogBuilderResult = Option<Result<Trade, Box<dyn Error>>>;

pub struct EntryDialogBuilder {
    account: Option<Account>,
    trade: Option<Trade>,
    result: EntryDialogBuilderResult,
}

impl EntryDialogBuilder {
    pub fn new() -> Self {
        EntryDialogBuilder {
            account: None,
            trade: None,
            result: None,
        }
    }

    pub fn build(mut self, trust: &mut Trust) -> EntryDialogBuilder {
        let trade: Trade = self.trade.clone().unwrap();
        self.result = Some(trust.record_entry(&trade));
        self
    }

    pub fn display(self) {
        match self
            .result
            .expect("No result found, did you forget to call search?")
        {
            Ok(trade) => {
                println!("Trade entry executed:");
                TradeView::display_trade(&trade, &self.account.unwrap().name);
                println!("Trade overview:");
                TradeOverviewView::display(trade.overview);
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
        let trades =
            trust.search_all_approved_trades_waiting_for_entry(self.account.clone().unwrap().id);
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