use crate::dialogs::AccountSearchDialog;
use crate::views::{LogView, OrderView, TradeOverviewView, TradeView};
use core::TrustFacade;
use dialoguer::{theme::ColorfulTheme, FuzzySelect, Input};
use model::{Account, BrokerLog, Status, Trade};
use rust_decimal::Decimal;
use std::error::Error;

type ModifyStopDialogBuilderResult = Option<Result<(Trade, BrokerLog), Box<dyn Error>>>;

pub struct ModifyStopDialogBuilder {
    account: Option<Account>,
    trade: Option<Trade>,
    new_stop_price: Option<Decimal>,
    result: ModifyStopDialogBuilderResult,
}

impl ModifyStopDialogBuilder {
    pub fn new() -> Self {
        ModifyStopDialogBuilder {
            account: None,
            trade: None,
            new_stop_price: None,
            result: None,
        }
    }

    pub fn build(mut self, trust: &mut TrustFacade) -> ModifyStopDialogBuilder {
        let trade = self
            .trade
            .clone()
            .expect("No trade found, did you forget to call search?");

        let account = self
            .account
            .clone()
            .expect("No account found, did you forget to call account?");
        let stop_price = self
            .new_stop_price
            .expect("No stop price found, did you forget to call stop_price?");

        match trust.modify_stop(&trade, &account, stop_price) {
            Ok((trade, log)) => self.result = Some(Ok((trade, log))),
            Err(error) => self.result = Some(Err(error)),
        }
        self
    }

    pub fn display(self) {
        match self
            .result
            .expect("No result found, did you forget to call search?")
        {
            Ok((trade, log)) => {
                println!("Trade stop updated:");
                TradeView::display(&trade, &self.account.unwrap().name);

                TradeOverviewView::display(&trade.overview);

                println!("Stop updated:");
                OrderView::display(trade.safety_stop);

                LogView::display(&log);
            }
            Err(error) => println!("Error submitting trade: {:?}", error),
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
        let trades = trust.search_trades(self.account.clone().unwrap().id, Status::Filled);
        match trades {
            Ok(trades) => {
                if trades.is_empty() {
                    panic!("No trade found with the status filled, did you forget to submit one?")
                }
                let trade = FuzzySelect::with_theme(&ColorfulTheme::default())
                    .with_prompt("Trade:")
                    .items(&trades[..])
                    .default(0)
                    .interact_opt()
                    .unwrap()
                    .map(|index| trades.get(index).unwrap())
                    .unwrap();

                println!("Trade selected:");
                TradeView::display(trade, &self.account.clone().unwrap().name);
                self.trade = Some(trade.to_owned());
            }
            Err(error) => self.result = Some(Err(error)),
        }

        self
    }

    pub fn stop_price(mut self) -> Self {
        let stop_price = Input::new()
            .with_prompt("New stop price")
            .interact()
            .unwrap();
        self.new_stop_price = Some(stop_price);
        self
    }
}
