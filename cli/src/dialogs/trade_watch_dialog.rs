//! UI Dialog Module - Trade Watch (polling-based)
//!
//! TEMPORARY SAFETY ALLOWANCE: This dialog module contains user interaction code
//! that uses .unwrap() and .expect() for UI input handling.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use crate::dialogs::AccountSearchDialog;
use crate::views::{ExecutionView, OrderView};
use core::TrustFacade;
use dialoguer::{theme::ColorfulTheme, FuzzySelect};
use model::{Account, Execution, Status, Trade};
use std::collections::HashSet;
use std::error::Error;
use std::thread::sleep;
use std::time::Duration;

type WatchResult = Option<Result<(), Box<dyn Error>>>;

pub struct TradeWatchDialogBuilder {
    account: Option<Account>,
    trade: Option<Trade>,
    result: WatchResult,
}

impl TradeWatchDialogBuilder {
    pub fn new() -> Self {
        Self {
            account: None,
            trade: None,
            result: None,
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

    pub fn search(mut self, trust: &mut TrustFacade) -> Self {
        let account = self.account.clone().unwrap();
        let mut trades = trust.search_trades(account.id, Status::Submitted).unwrap();
        trades.append(&mut trust.search_trades(account.id, Status::Filled).unwrap());
        trades.append(
            &mut trust
                .search_trades(account.id, Status::PartiallyFilled)
                .unwrap(),
        );

        if trades.is_empty() {
            panic!("No trade found with status Submitted / PartiallyFilled / Filled");
        }

        let trade = FuzzySelect::with_theme(&ColorfulTheme::default())
            .with_prompt("Trade to watch:")
            .items(&trades[..])
            .default(0)
            .interact_opt()
            .unwrap()
            .map(|index| trades.get(index).unwrap())
            .unwrap();

        self.trade = Some(trade.to_owned());
        self
    }

    pub fn build(mut self, trust: &mut TrustFacade) -> Self {
        let trade = self.trade.clone().expect("No trade selected");
        let account = self.account.clone().expect("No account selected");

        self.result = Some(watch_loop(trust, &trade, &account));
        self
    }

    pub fn display(self) {
        match self
            .result
            .expect("No result found, did you forget to call build()?")
        {
            Ok(()) => {}
            Err(error) => println!("Trade watch error: {error:?}"),
        }
    }
}

fn terminal_status(status: Status) -> bool {
    matches!(
        status,
        Status::ClosedTarget
            | Status::ClosedStopLoss
            | Status::Canceled
            | Status::Expired
            | Status::Rejected
    )
}

fn watch_loop(
    trust: &mut TrustFacade,
    trade: &Trade,
    account: &Account,
) -> Result<(), Box<dyn Error>> {
    let mut seen_execution_ids: HashSet<String> = HashSet::new();
    let mut last_status: Option<Status> = None;

    println!("Watching trade {} (Ctrl-C to stop)", trade.id);

    loop {
        let (status, orders, _log) = trust.sync_trade(trade, account)?;

        if last_status != Some(status) {
            println!();
            println!("Status: {status:?}");
            last_status = Some(status);
        }

        if !orders.is_empty() {
            println!();
            println!("Updated orders:");
            OrderView::display_orders(orders);
        }

        let executions: Vec<Execution> = trust.executions_for_trade(trade.id)?;
        let new_execs: Vec<Execution> = executions
            .into_iter()
            .filter(|e| seen_execution_ids.insert(e.broker_execution_id.clone()))
            .collect();
        if !new_execs.is_empty() {
            println!();
            println!("New executions:");
            ExecutionView::display(&new_execs);
        }

        if terminal_status(status) {
            println!();
            println!("Trade reached terminal status: {status:?}");
            break;
        }

        sleep(Duration::from_secs(2));
    }

    Ok(())
}
