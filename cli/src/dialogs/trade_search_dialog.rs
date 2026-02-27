//! UI Dialog Module - User Interaction Code
//!
//! TEMPORARY SAFETY ALLOWANCE: This dialog module contains user interaction code
//! that uses .unwrap() and .expect() for UI input handling. While not ideal,
//! these are less critical than business logic safety violations.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use core::TrustFacade;
use model::{Account, Status, Trade};

use crate::views::{OrderView, TradeView};
use crate::{dialogs::AccountSearchDialog, views::TradeBalanceView};
use crate::{dialogs::ConsoleDialogIo, dialogs::DialogIo};
use std::error::Error;

pub struct TradeSearchDialogBuilder {
    account: Option<Account>,
    status: Option<Status>,
    balance: bool,
    result: Option<Result<Vec<Trade>, Box<dyn Error>>>,
}

impl TradeSearchDialogBuilder {
    pub fn new() -> Self {
        TradeSearchDialogBuilder {
            result: None,
            account: None,
            balance: true,
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
                let name = self.account.clone().unwrap().name;

                if self.balance {
                    println!("Trades found:");
                    for trade in trades {
                        TradeView::display(&trade, name.as_str());
                        TradeBalanceView::display(&trade.balance);
                        println!("Entry:");
                        OrderView::display(trade.entry);
                        println!("Target:");
                        OrderView::display(trade.target);
                        println!("Stop:");
                        OrderView::display(trade.safety_stop);
                    }
                } else {
                    println!("Trades found:");
                    TradeView::display_trades(trades, name.as_str());
                }
            }
            Err(error) => println!("Error searching account: {error:?}"),
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
        self.result =
            Some(trust.search_trades(self.account.clone().unwrap().id, self.status.unwrap()));
        self
    }

    pub fn status(self) -> Self {
        let mut io = ConsoleDialogIo::default();
        self.status_with_io(&mut io)
    }

    pub fn status_with_io(mut self, io: &mut dyn DialogIo) -> Self {
        let available = Status::all();
        let labels = available
            .iter()
            .map(std::string::ToString::to_string)
            .collect::<Vec<_>>();
        if let Ok(Some(index)) = io.select_index("Status:", &labels, 0) {
            self.status = available.get(index).copied();
        }
        self
    }

    pub fn show_balance(self) -> Self {
        let mut io = ConsoleDialogIo::default();
        self.show_balance_with_io(&mut io)
    }

    pub fn show_balance_with_io(mut self, io: &mut dyn DialogIo) -> Self {
        if let Ok(value) = io.confirm("Do you want to see details form each trade?", true) {
            self.balance = value;
        }
        self
    }
}

#[cfg(test)]
mod tests {
    use super::TradeSearchDialogBuilder;
    use crate::dialogs::DialogIo;
    use alpaca_broker::AlpacaBroker;
    use core::TrustFacade;
    use db_sqlite::SqliteDatabase;
    use model::Status;
    use model::{Account, Environment, Trade};
    use rust_decimal_macros::dec;
    use std::collections::VecDeque;
    use std::io::{Error as IoError, ErrorKind};
    use uuid::Uuid;

    #[derive(Default)]
    struct ScriptedIo {
        selections: VecDeque<Result<Option<usize>, IoError>>,
        confirmations: VecDeque<Result<bool, IoError>>,
    }

    impl DialogIo for ScriptedIo {
        fn select_index(
            &mut self,
            _prompt: &str,
            _labels: &[String],
            _default: usize,
        ) -> Result<Option<usize>, IoError> {
            self.selections.pop_front().unwrap_or(Ok(None))
        }

        fn confirm(&mut self, _prompt: &str, _default: bool) -> Result<bool, IoError> {
            self.confirmations.pop_front().unwrap_or(Ok(false))
        }
    }

    fn test_trust() -> TrustFacade {
        let path = std::env::temp_dir().join(format!("trust-test-{}.db", Uuid::new_v4()));
        let db = SqliteDatabase::new(path.to_str().expect("valid temp db path"));
        TrustFacade::new(Box::new(db), Box::<AlpacaBroker>::default())
    }

    #[test]
    fn status_with_io_sets_expected_status() {
        let mut io = ScriptedIo::default();
        io.selections.push_back(Ok(Some(0)));
        let dialog = TradeSearchDialogBuilder::new().status_with_io(&mut io);
        assert_eq!(dialog.status, Some(Status::New));
    }

    #[test]
    fn status_with_io_keeps_none_when_cancelled() {
        let mut io = ScriptedIo::default();
        io.selections.push_back(Ok(None));
        let dialog = TradeSearchDialogBuilder::new().status_with_io(&mut io);
        assert_eq!(dialog.status, None);
    }

    #[test]
    fn show_balance_with_io_sets_preference() {
        let mut io = ScriptedIo::default();
        io.confirmations.push_back(Ok(true));
        let dialog = TradeSearchDialogBuilder::new().show_balance_with_io(&mut io);
        assert!(dialog.balance);
    }

    #[test]
    fn show_balance_with_io_preserves_default_on_error() {
        let mut io = ScriptedIo::default();
        io.confirmations
            .push_back(Err(IoError::new(ErrorKind::Interrupted, "cancelled")));
        let dialog = TradeSearchDialogBuilder::new().show_balance_with_io(&mut io);
        assert!(dialog.balance);
    }

    #[test]
    fn display_handles_empty_and_success_paths() {
        let account = Account {
            name: "paper".to_string(),
            ..Account::default()
        };

        TradeSearchDialogBuilder {
            account: Some(account.clone()),
            status: Some(Status::New),
            balance: true,
            result: Some(Ok(vec![])),
        }
        .display();

        TradeSearchDialogBuilder {
            account: Some(account.clone()),
            status: Some(Status::New),
            balance: true,
            result: Some(Ok(vec![Trade::default()])),
        }
        .display();

        TradeSearchDialogBuilder {
            account: Some(account),
            status: Some(Status::New),
            balance: false,
            result: Some(Ok(vec![Trade::default()])),
        }
        .display();
    }

    #[test]
    fn search_returns_empty_list_for_account_with_no_matching_status() {
        let mut trust = test_trust();
        let account = trust
            .create_account(
                "search-status",
                "desc",
                Environment::Paper,
                dec!(20),
                dec!(10),
            )
            .expect("account should be created");

        let dialog = TradeSearchDialogBuilder {
            account: Some(account),
            status: Some(Status::New),
            balance: true,
            result: None,
        }
        .search(&mut trust);

        let trades = dialog
            .result
            .expect("result should be set")
            .expect("search should succeed");
        assert!(trades.is_empty());
    }

    #[test]
    #[should_panic]
    fn search_panics_when_account_or_status_missing() {
        let mut trust = test_trust();
        let _ = TradeSearchDialogBuilder::new().search(&mut trust);
    }
}
