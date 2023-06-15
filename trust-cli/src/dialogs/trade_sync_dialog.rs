use crate::dialogs::AccountSearchDialog;
use crate::views::{LogView, OrderView};
use dialoguer::{theme::ColorfulTheme, FuzzySelect};
use std::error::Error;
use trust_core::TrustFacade;
use trust_model::{Account, BrokerLog, Order, Status, Trade};

type EntryDialogBuilderResult = Option<Result<(Status, Vec<Order>, BrokerLog), Box<dyn Error>>>;

pub struct SyncTradeDialogBuilder {
    account: Option<Account>,
    trade: Option<Trade>,
    result: EntryDialogBuilderResult,
}

impl SyncTradeDialogBuilder {
    pub fn new() -> Self {
        SyncTradeDialogBuilder {
            account: None,
            trade: None,
            result: None,
        }
    }

    pub fn build(mut self, trust: &mut TrustFacade) -> SyncTradeDialogBuilder {
        let trade: Trade = self
            .trade
            .clone()
            .expect("No trade found, did you forget to select one?");
        self.result = Some(trust.sync_trade(&trade, &self.account.clone().unwrap()));
        self
    }

    pub fn display(self) {
        match self
            .result
            .expect("No result found, did you forget to call search?")
        {
            Ok((status, orders, log)) => {
                if orders.is_empty() {
                    println!(
                        "All orders from trade {} are up to date",
                        self.trade.unwrap().id
                    );
                    return;
                }

                println!("Trade synced, the status is: {:?}", status);
                println!();
                println!("Updated orders:");
                OrderView::display_orders(orders);

                println!("Logs:");
                LogView::display(&log);
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
        // We need to search for trades with status Submitted and Filled to find the trade we want to sync
        let mut trades = trust
            .search_trades(self.account.clone().unwrap().id, Status::Submitted)
            .unwrap();
        trades.append(
            &mut trust
                .search_trades(self.account.clone().unwrap().id, Status::Filled)
                .unwrap(),
        );

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
        self
    }
}
