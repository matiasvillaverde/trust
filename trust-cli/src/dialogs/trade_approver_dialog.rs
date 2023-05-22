use crate::dialogs::AccountSearchDialog;
use crate::views::TradeView;
use dialoguer::{theme::ColorfulTheme, FuzzySelect};
use std::error::Error;
use trust_core::Trust;
use trust_model::{Account, Trade};

pub struct TradeDialogApproverBuilder {
    account: Option<Account>,
    trade: Option<Trade>,
    result: Option<Result<Trade, Box<dyn Error>>>,
}

impl TradeDialogApproverBuilder {
    pub fn new() -> Self {
        TradeDialogApproverBuilder {
            account: None,
            trade: None,
            result: None,
        }
    }

    pub fn build(self) -> TradeDialogApproverBuilder {
        // TODO: Run all the rules
        // 1. Reject in case a rule fails
        // 2. Approve in case rule succeed

        // TODO: Create a transaction to fund the trade
        assert!(self.result.is_some());
        self
    }

    pub fn display(self) {
        match self
            .result
            .expect("No result found, did you forget to call search?")
        {
            Ok(trade) => TradeView::display_trade(&trade, &self.account.unwrap().name),
            Err(error) => println!("Error creating trade: {:?}", error),
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
        let trades = trust.search_all_new_trades(self.account.clone().unwrap().id);
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
