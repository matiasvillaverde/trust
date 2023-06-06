use crate::dialogs::AccountSearchDialog;
use crate::views::{TradeOverviewView, TradeView};
use dialoguer::{theme::ColorfulTheme, FuzzySelect};
use std::error::Error;
use trust_core::TrustFacade;
use trust_model::{Account, BrokerLog, Order, Trade};

type TradeDialogApproverBuilderResult = Option<Result<(Trade, Order, BrokerLog), Box<dyn Error>>>;

pub struct SubmitDialogBuilder {
    account: Option<Account>,
    trade: Option<Trade>,
    result: TradeDialogApproverBuilderResult,
}

impl SubmitDialogBuilder {
    pub fn new() -> Self {
        SubmitDialogBuilder {
            account: None,
            trade: None,
            result: None,
        }
    }

    pub fn build(mut self, trust: &mut TrustFacade) -> SubmitDialogBuilder {
        let trade: Trade = self.trade.clone().unwrap();
        self.result = Some(trust.submit_trade(&trade));
        self
    }

    pub fn display(self) {
        match self
            .result
            .expect("No result found, did you forget to call search?")
        {
            Ok((trade, order, log)) => {
                println!("Trade submitted:");
                TradeView::display_trade(&trade, &self.account.unwrap().name);

                println!("Trade overview:");
                TradeOverviewView::display(&trade.overview);

                // println!("Order:");
                // OrderView::display(&order); // TODO: Display Order and logs

                // println!("Broker log:");
                // BrokerLogView::display(&log);
            }
            Err(error) => println!("Error funding trade: {:?}", error),
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
