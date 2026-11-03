//! UI Dialog Module - Trade Watch (polling-based)
//!
//! TEMPORARY SAFETY ALLOWANCE: This dialog module contains user interaction code.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use crate::dialogs::{dialog_helpers, AccountSearchDialog, ConsoleDialogIo};
use crate::views::{ExecutionView, OrderView};
use broker_sync::{BrokerCommand, BrokerEvent, TradeSessionTracker};
use core::TrustFacade;
use model::{Account, Execution, Status, Trade};
use std::collections::HashSet;
use std::error::Error;
use std::io::ErrorKind;
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
        let account = match dialog_helpers::require(
            self.account.clone(),
            ErrorKind::InvalidInput,
            "No account selected",
        ) {
            Ok(account) => account,
            Err(error) => {
                self.result = Some(Err(error));
                return self;
            }
        };

        let trades = match open_trades_for_account(trust, account.id) {
            Ok(trades) => trades,
            Err(error) => {
                self.result = Some(Err(error));
                return self;
            }
        };

        let mut io = ConsoleDialogIo::default();
        match dialog_helpers::select_from_list(
            &mut io,
            "Trade to watch:",
            &trades,
            "No trade found with status Submitted / PartiallyFilled / Filled",
            "Trade selection was canceled",
        ) {
            Ok(trade) => self.trade = Some(trade),
            Err(error) => self.result = Some(Err(error)),
        }

        self
    }

    pub fn latest(mut self, trust: &mut TrustFacade) -> Self {
        let account = match dialog_helpers::require(
            self.account.clone(),
            ErrorKind::InvalidInput,
            "No account selected",
        ) {
            Ok(account) => account,
            Err(error) => {
                self.result = Some(Err(error));
                return self;
            }
        };
        let mut trades = match open_trades_for_account(trust, account.id) {
            Ok(trades) => trades,
            Err(error) => {
                self.result = Some(Err(error));
                return self;
            }
        };

        trades.sort_by_key(|trade| trade.updated_at);
        let trade = match trades.pop() {
            Some(trade) => trade,
            None => {
                self.result = Some(Err(Box::new(std::io::Error::new(
                    ErrorKind::NotFound,
                    "No trade found with status Submitted / PartiallyFilled / Filled",
                ))));
                return self;
            }
        };
        self.trade = Some(trade);
        self
    }

    pub fn build(mut self, trust: &mut TrustFacade) -> Self {
        if self.result.as_ref().is_some_and(Result::is_err) {
            return self;
        }
        let trade = match dialog_helpers::require(
            self.trade.clone(),
            ErrorKind::InvalidInput,
            "No trade selected",
        ) {
            Ok(trade) => trade,
            Err(error) => {
                self.result = Some(Err(error));
                return self;
            }
        };
        let account = match dialog_helpers::require(
            self.account.clone(),
            ErrorKind::InvalidInput,
            "No account selected",
        ) {
            Ok(account) => account,
            Err(error) => {
                self.result = Some(Err(error));
                return self;
            }
        };

        self.result = Some(watch_loop(trust, &trade, &account));
        self
    }

    pub fn display(self) {
        match self.result {
            Some(Ok(())) => {}
            Some(Err(error)) => println!("Trade watch error: {error:?}"),
            None => println!("No result found, did you forget to call build()?"),
        }
    }
}

fn open_trades_for_account(
    trust: &mut TrustFacade,
    account_id: uuid::Uuid,
) -> Result<Vec<Trade>, Box<dyn Error>> {
    open_trades_for_account_with(account_id, |account_id, status| {
        trust.search_trades(account_id, status)
    })
}

fn open_trades_for_account_with(
    account_id: uuid::Uuid,
    mut search: impl FnMut(uuid::Uuid, Status) -> Result<Vec<Trade>, Box<dyn Error>>,
) -> Result<Vec<Trade>, Box<dyn Error>> {
    let mut trades = search(account_id, Status::Submitted)?;
    trades.append(&mut search(account_id, Status::Filled)?);
    trades.append(&mut search(account_id, Status::PartiallyFilled)?);
    Ok(trades)
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
    let session_actor = TradeSessionTracker::spawn();
    session_actor.send(BrokerCommand::StartTradeSession {
        account_id: account.id,
        trade_id: trade.id,
    })?;
    let _ = session_actor.recv_timeout(Duration::from_millis(200));

    let mut seen_execution_ids: HashSet<String> = HashSet::new();
    let mut last_status: Option<Status> = None;

    println!("Watching trade {} (Ctrl-C to stop)", trade.id);
    let mut current_trade = trade.clone();

    loop {
        let (status, orders, _log) = trust.sync_trade(&current_trade, account)?;
        let _ = session_actor.send(BrokerCommand::TouchTradeSession { trade_id: trade.id });
        current_trade = refresh_trade_snapshot(trust, account.id, trade.id, status)
            .unwrap_or_else(|| current_trade.clone());

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

        let executions: Vec<Execution> = trust.executions_for_trade(current_trade.id)?;
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
            let _ = session_actor.send(BrokerCommand::StopTradeSession { trade_id: trade.id });
            if let Ok(BrokerEvent::TradeSessionStopped { .. }) =
                session_actor.recv_timeout(Duration::from_millis(200))
            {
                println!("Managed session stopped.");
            }
            let _ = session_actor.send(BrokerCommand::Shutdown);
            break;
        }

        sleep(watch_poll_delay());
    }

    Ok(())
}

#[cfg(test)]
fn watch_poll_delay() -> Duration {
    Duration::from_millis(1)
}

#[cfg(not(test))]
fn watch_poll_delay() -> Duration {
    Duration::from_secs(2)
}

fn refresh_trade_snapshot(
    trust: &mut TrustFacade,
    account_id: uuid::Uuid,
    trade_id: uuid::Uuid,
    status: Status,
) -> Option<Trade> {
    refresh_trade_snapshot_with(account_id, trade_id, status, |account_id, status| {
        trust.search_trades(account_id, status)
    })
}

fn refresh_trade_snapshot_with(
    account_id: uuid::Uuid,
    trade_id: uuid::Uuid,
    status: Status,
    mut search: impl FnMut(uuid::Uuid, Status) -> Result<Vec<Trade>, Box<dyn Error>>,
) -> Option<Trade> {
    search(account_id, status)
        .ok()?
        .into_iter()
        .find(|trade| trade.id == trade_id)
}

#[cfg(test)]
mod tests;
