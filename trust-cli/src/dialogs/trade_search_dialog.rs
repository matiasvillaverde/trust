use trust_core::TrustFacade;
use trust_model::{Account, Status, Trade};

use crate::views::TradeView;
use crate::{dialogs::AccountSearchDialog, views::TradeOverviewView};
use dialoguer::{theme::ColorfulTheme, Confirm, FuzzySelect};
use std::error::Error;

pub struct TradeSearchDialogBuilder {
    account: Option<Account>,
    status: Option<Status>,
    overview: bool,
    result: Option<Result<Vec<Trade>, Box<dyn Error>>>,
}

impl TradeSearchDialogBuilder {
    pub fn new() -> Self {
        TradeSearchDialogBuilder {
            result: None,
            account: None,
            overview: true,
            status: None,
        }
    }

    pub fn display(self) {
        match self
            .result
            .expect("No result found, did you forget to call search?")
        {
            Ok(trades) => {
                if trades.is_empty() {
                    println!("No trades found");
                    return;
                }

                if self.overview {
                    println!("Trades found:");
                    let name = self.account.clone().unwrap().name;
                    for trade in trades {
                        println!("Trade: {}", trade.id);
                        TradeView::display(&trade, name.as_str());
                        println!("Overview:");
                        TradeOverviewView::display(&trade.overview);
                    }
                } else {
                    println!("Trades found:");
                    TradeView::display_trades(trades, self.account.unwrap().name.as_str());
                }
            }
            Err(error) => println!("Error searching account: {:?}", error),
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
        self.result =
            Some(trust.search_trades(self.account.clone().unwrap().id, self.status.unwrap()));
        self
    }

    pub fn status(mut self) -> Self {
        let available = Status::all();

        let status: &Status = FuzzySelect::with_theme(&ColorfulTheme::default())
            .with_prompt("Status:")
            .items(&available[..])
            .interact()
            .map(|index| available.get(index).unwrap())
            .unwrap();

        self.status = Some(*status);
        self
    }

    pub fn show_overview(mut self) -> Self {
        if Confirm::with_theme(&ColorfulTheme::default())
            .with_prompt("Do you want to see details form each trade?")
            .default(true)
            .interact()
            .unwrap()
        {
            self.overview = true;
        } else {
            self.overview = false;
        }
        self
    }
}
