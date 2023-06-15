use crate::dialogs::AccountSearchDialog;
use crate::views::{AccountOverviewView, TradeOverviewView, TradeView, TransactionView};
use dialoguer::{theme::ColorfulTheme, FuzzySelect, Input};
use rust_decimal::Decimal;
use std::error::Error;
use trust_core::TrustFacade;
use trust_model::{Account, AccountOverview, Status, Trade, TradeOverview, Transaction};

type ExitDialogBuilderResult =
    Option<Result<(Transaction, Transaction, TradeOverview, AccountOverview), Box<dyn Error>>>;

pub struct ExitDialogBuilder {
    account: Option<Account>,
    trade: Option<Trade>,
    fee: Option<Decimal>,
    result: ExitDialogBuilderResult,
}

impl ExitDialogBuilder {
    pub fn new() -> Self {
        ExitDialogBuilder {
            account: None,
            trade: None,
            fee: None,
            result: None,
        }
    }

    pub fn build_stop(mut self, trust: &mut TrustFacade) -> ExitDialogBuilder {
        let trade: Trade = self
            .trade
            .clone()
            .expect("No trade found, did you forget to select one?");
        let fee = self
            .fee
            .expect("No fee found, did you forget to specify a fee?");
        self.result = Some(trust.stop_trade(&trade, fee));
        self
    }

    pub fn build_target(mut self, trust: &mut TrustFacade) -> ExitDialogBuilder {
        let trade: Trade = self
            .trade
            .clone()
            .expect("No trade found, did you forget to select one?");
        let fee = self
            .fee
            .expect("No fee found, did you forget to specify a fee?");
        self.result = Some(trust.target_acquired(&trade, fee));
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
                TradeView::display(&self.trade.unwrap(), account_name.as_str());

                println!("With transaction of exit:");
                TransactionView::display(&tx_exit, account_name.as_str());

                println!("With transaction of payment back to the account:");
                TransactionView::display(&tx_payment, account_name.as_str());

                println!("Trade overview:");
                TradeOverviewView::display(&trade_overview);

                println!("Account overview:");
                AccountOverviewView::display(account_overview, account_name.as_str());
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
        let trades = trust.search_trades(self.account.clone().unwrap().id, Status::Filled);
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

    pub fn fee(mut self) -> Self {
        let fee_price = Input::new().with_prompt("Fee price").interact().unwrap(); // TODO: Validate
        self.fee = Some(fee_price);
        self
    }
}
