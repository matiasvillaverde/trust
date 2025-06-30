use crate::dialogs::AccountSearchDialog;
use crate::views::{TradeBalanceView, TradeView, TransactionView};
use core::TrustFacade;
use dialoguer::{theme::ColorfulTheme, FuzzySelect, Input};
use model::{Account, Status, Trade, Transaction};
use rust_decimal::Decimal;
use std::error::Error;

type EntryDialogBuilderResult = Option<Result<(Trade, Transaction), Box<dyn Error>>>;

pub struct FillTradeDialogBuilder {
    account: Option<Account>,
    trade: Option<Trade>,
    fee: Option<Decimal>,
    result: EntryDialogBuilderResult,
}

impl FillTradeDialogBuilder {
    pub fn new() -> Self {
        FillTradeDialogBuilder {
            account: None,
            trade: None,
            fee: None,
            result: None,
        }
    }

    pub fn build(mut self, trust: &mut TrustFacade) -> FillTradeDialogBuilder {
        let trade: Trade = self
            .trade
            .clone()
            .expect("No trade found, did you forget to select one?");
        let fee = self
            .fee
            .expect("No fee found, did you forget to specify a fee?");
        self.result = Some(trust.fill_trade(&trade, fee));
        self
    }

    pub fn display(self) {
        match self
            .result
            .expect("No result found, did you forget to call search?")
        {
            Ok((trade, tx)) => {
                let name = self.account.unwrap().name;
                println!("Trade entry executed:");
                TradeView::display(&trade, name.as_str());
                TradeBalanceView::display(&trade.balance);
                TransactionView::display(&tx, name.as_str());
            }
            Err(error) => println!("Error approving trade: {error:?}"),
        }
    }

    pub fn account(mut self, trust: &mut TrustFacade) -> Self {
        let account = AccountSearchDialog::new().search(trust).build();
        match account {
            Ok(account) => self.account = Some(account),
            Err(error) => println!("Error searching account: {error:?}"),
        }
        self
    }

    pub fn fee(mut self) -> Self {
        let fee_price = Input::with_theme(&ColorfulTheme::default())
            .with_prompt("Fee")
            .validate_with({
                |input: &String| -> Result<(), &str> {
                    match input.parse::<Decimal>() {
                        Ok(parsed) => {
                            if parsed.is_sign_negative() {
                                return Err("Please enter a positive fee");
                            }
                            Ok(())
                        }
                        Err(_) => Err("Please enter a valid number for the fee"),
                    }
                }
            })
            .interact_text()
            .unwrap()
            .parse::<Decimal>()
            .unwrap();

        self.fee = Some(fee_price);
        self
    }

    pub fn search(mut self, trust: &mut TrustFacade) -> Self {
        let trades = trust.search_trades(self.account.clone().unwrap().id, Status::Submitted);
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
