use trust_core::TrustFacade;
use trust_model::{Account, Status, Trade};

use crate::dialogs::AccountSearchDialog;
use crate::views::TradeView;
use dialoguer::{theme::ColorfulTheme, FuzzySelect};
use std::error::Error;

pub struct TradeSearchDialogBuilder {
    account: Option<Account>,
    status: Option<Status>,
    result: Option<Result<Vec<Trade>, Box<dyn Error>>>,
}

impl TradeSearchDialogBuilder {
    pub fn new() -> Self {
        TradeSearchDialogBuilder {
            result: None,
            account: None,
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
                println!("Trades found:");
                TradeView::display_trades(trades, self.account.unwrap().name.as_str());
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
}
