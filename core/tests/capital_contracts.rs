//! Cash contracts through the public facade and real persistence. Expected balances
//! are independent of Trust's calculators; only storage fault injection uses SQL.
use core::TrustFacade;
use db_sqlite::SqliteDatabase;
use diesel::{Connection, RunQueryDsl};
use model::{
    Account, AccountBalance, AccountType, Currency, DistributionHistory, Environment, Transaction,
    TransactionCategory,
};
use proptest::prelude::*;
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use uuid::Uuid;

struct Ledger {
    trust: TrustFacade,
    directory: tempfile::TempDir,
    source: Uuid,
    children: [Uuid; 4],
    unrelated: Uuid,
}

#[derive(Debug, PartialEq)]
struct Snapshot {
    balances: Vec<AccountBalance>,
    transactions: Vec<Transaction>,
    distributions: Vec<DistributionHistory>,
}

impl Ledger {
    fn new() -> Self {
        let directory = tempfile::tempdir().unwrap();
        // These are local ledger operations: no broker or credentials are configured.
        let mut trust = Self::open(&directory);
        let source = trust
            .create_account("primary", "capital", Environment::Paper, dec!(0), dec!(0))
            .unwrap();
        let children = [
            AccountType::Earnings,
            AccountType::TaxReserve,
            AccountType::Reinvestment,
            AccountType::Insurance,
        ]
        .map(|kind| {
            trust
                .create_account_with_hierarchy(
                    &format!("{kind:?}"),
                    "reserve",
                    Environment::Paper,
                    dec!(0),
                    dec!(0),
                    kind,
                    Some(source.id),
                )
                .unwrap()
                .id
        });
        let unrelated = trust
            .create_account(
                "unrelated",
                "separate capital",
                Environment::Paper,
                dec!(0),
                dec!(0),
            )
            .unwrap()
            .id;
        for (currency, amount) in [
            (Currency::USD, dec!(1000)),
            (Currency::EUR, dec!(2000)),
            (Currency::BTC, dec!(3.5)),
        ] {
            trust
                .create_transaction(&source, &TransactionCategory::Deposit, amount, &currency)
                .unwrap();
        }
        Self {
            trust,
            directory,
            source: source.id,
            children,
            unrelated,
        }
    }

    fn open(directory: &tempfile::TempDir) -> TrustFacade {
        let path = directory.path().join("capital.db");
        TrustFacade::new_with_brokers(
            Box::new(SqliteDatabase::new(path.to_str().unwrap())),
            vec![],
        )
    }

    fn reopen(&mut self) {
        self.trust = Self::open(&self.directory);
    }

    fn accounts(&mut self) -> Vec<Account> {
        let mut accounts = self.trust.search_all_accounts().unwrap();
        accounts.sort_by_key(|account| account.id);
        accounts
    }

    fn snapshot(&mut self) -> Snapshot {
        let mut state = Snapshot {
            balances: vec![],
            transactions: vec![],
            distributions: vec![],
        };
        for account in self.accounts() {
            state
                .balances
                .extend(self.trust.search_all_balances(account.id).unwrap());
            state
                .transactions
                .extend(self.trust.get_account_transactions(account.id).unwrap());
            state
                .distributions
                .extend(self.trust.distribution_history(account.id).unwrap());
        }
        state.balances.sort_by_key(|balance| balance.id);
        state.transactions.sort_by_key(|transaction| transaction.id);
        state
            .distributions
            .sort_by_key(|distribution| distribution.id);
        state
    }

    fn cash(&mut self, account: Uuid, currency: Currency) -> Decimal {
        // A new reserve account can legitimately have no balance in this currency.
        // Propagate read errors; do not turn a failed read into a zero balance.
        let balances = self.trust.search_all_balances(account).unwrap();
        let balance = balances.iter().find(|balance| balance.currency == currency);
        if let Some(balance) = balance {
            assert_eq!(balance.total_in_trade, Decimal::ZERO);
            assert_eq!(balance.total_balance, balance.total_available);
            balance.total_available
        } else {
            Decimal::ZERO
        }
    }

    fn configure_distribution(&mut self, minimum: Decimal) {
        self.trust
            .configure_distribution_with_insurance(
                self.source,
                dec!(0.4),
                dec!(0.3),
                dec!(0.2),
                dec!(0.1),
                minimum,
                "capital-contract-fixture",
            )
            .unwrap();
    }

    fn sql(&self, sql: &str) {
        let path = self.directory.path().join("capital.db");
        let mut connection = diesel::SqliteConnection::establish(path.to_str().unwrap()).unwrap();
        diesel::sql_query(sql).execute(&mut connection).unwrap();
    }

    fn fail_writes(&self, event: &str, table: &str, predicate: &str) {
        // SQL is confined to injecting a failure at a persistence boundary. The
        // behavioral oracle above uses public account/ledger/history queries.
        self.sql(&format!("CREATE TRIGGER cash_failure BEFORE {event} ON {table} {predicate} BEGIN SELECT RAISE(ABORT, 'injected cash write failure'); END"));
    }
}

#[test]
fn transfers_move_exact_cash_between_related_accounts_and_survive_restart() {
    let mut ledger = Ledger::new();
    let [earnings, tax, ..] = ledger.children;
    for (currency, opening, amount) in [
        (Currency::USD, dec!(1000), dec!(125.37)),
        (Currency::EUR, dec!(2000), dec!(2000)),
        (Currency::BTC, dec!(3.5), dec!(0.00000001)),
    ] {
        let before = ledger.snapshot();
        // Parent -> child, siblings, and child -> parent are all supported.
        for (from, to) in [
            (ledger.source, earnings),
            (earnings, tax),
            (tax, ledger.source),
        ] {
            let from_before = ledger.cash(from, currency);
            let to_before = ledger.cash(to, currency);
            let (debit, credit) = ledger
                .trust
                .transfer_between_accounts(from, to, amount, currency, "move reserve capital")
                .unwrap();
            assert_eq!(ledger.cash(from, currency), from_before - amount);
            assert_eq!(ledger.cash(to, currency), to_before + amount);
            let transactions = ledger.trust.get_all_transactions().unwrap();
            for (id, account, category) in [
                (debit, from, TransactionCategory::Withdrawal),
                (credit, to, TransactionCategory::Deposit),
            ] {
                let receipt = transactions.iter().find(|tx| tx.id == id).unwrap();
                assert_eq!(
                    (
                        receipt.account_id,
                        receipt.currency,
                        receipt.category,
                        receipt.amount
                    ),
                    (account, currency, category, amount)
                );
            }
        }
        assert_eq!(ledger.cash(ledger.source, currency), opening);
        assert_eq!(ledger.cash(earnings, currency), Decimal::ZERO);
        assert_eq!(ledger.cash(tax, currency), Decimal::ZERO);
        let after = ledger.snapshot();
        assert_eq!(after.transactions.len(), before.transactions.len() + 6);
        // No accidental FX conversion or changes to another account/currency.
        let untouched = |snapshot: Snapshot| {
            snapshot
                .balances
                .into_iter()
                .filter(|balance| balance.currency != currency)
                .collect::<Vec<_>>()
        };
        assert_eq!(untouched(before), untouched(after));
    }
    let persisted = ledger.snapshot();
    ledger.reopen();
    assert_eq!(ledger.snapshot(), persisted);
}

#[test]
fn rejected_transfers_leave_cash_and_receipts_unchanged() {
    let mut ledger = Ledger::new();
    let destination = ledger.children[0];
    let before = ledger.snapshot();
    // Only one precondition is invalid in each case. In particular the
    // overdraft uses a related, valid destination, so hierarchy cannot mask it.
    for (to, amount) in [
        (ledger.source, dec!(1)),
        (ledger.unrelated, dec!(1)),
        (Uuid::new_v4(), dec!(1)),
        (destination, dec!(0)),
        (destination, dec!(-0.01)),
        (destination, dec!(1000.01)),
    ] {
        assert!(
            ledger
                .trust
                .transfer_between_accounts(
                    ledger.source,
                    to,
                    amount,
                    Currency::USD,
                    "rejected transfer",
                )
                .is_err(),
            "unexpectedly accepted {amount} to {to}"
        );
        assert_eq!(ledger.snapshot(), before);
    }
    // Exact available cash is allowed, despite the neighboring rejected case.
    ledger
        .trust
        .transfer_between_accounts(
            ledger.source,
            destination,
            dec!(1000),
            Currency::USD,
            "full balance",
        )
        .unwrap();
    assert_eq!(ledger.cash(ledger.source, Currency::USD), dec!(0));
    assert_eq!(ledger.cash(destination, Currency::USD), dec!(1000));
    assert_eq!(ledger.cash(ledger.source, Currency::EUR), dec!(2000));
    let drained = ledger.snapshot();
    assert!(ledger
        .trust
        .transfer_between_accounts(
            ledger.source,
            destination,
            dec!(0.01),
            Currency::USD,
            "other currencies cannot fund USD",
        )
        .is_err());
    assert_eq!(ledger.snapshot(), drained);
}

#[test]
fn failed_transfers_roll_back_both_accounts_and_can_be_retried_once() {
    for boundary in [
        "destination receipt",
        "source balance",
        "destination balance",
    ] {
        let mut ledger = Ledger::new();
        let destination = ledger.children[0];
        let before = ledger.snapshot();
        match boundary {
            "destination receipt" => ledger.fail_writes(
                "INSERT",
                "transactions",
                &format!("WHEN NEW.account_id = '{destination}'"),
            ),
            "source balance" => ledger.fail_writes("UPDATE", "accounts_balances", ""),
            _ => ledger.fail_writes(
                "INSERT",
                "accounts_balances",
                &format!("WHEN NEW.account_id = '{destination}'"),
            ),
        }
        let error = ledger
            .trust
            .transfer_between_accounts(
                ledger.source,
                destination,
                dec!(125.37),
                Currency::USD,
                "reserve capital",
            )
            .unwrap_err();
        assert!(
            error.to_string().contains("injected cash write failure"),
            "{boundary}: {error}"
        );
        assert_eq!(ledger.snapshot(), before, "partial transfer at {boundary}");
        ledger.sql("DROP TRIGGER cash_failure");
        ledger.reopen();
        assert_eq!(ledger.snapshot(), before, "rollback must survive reconnect");
        ledger
            .trust
            .transfer_between_accounts(
                ledger.source,
                destination,
                dec!(125.37),
                Currency::USD,
                "retry after verified rollback",
            )
            .unwrap();
        assert_eq!(ledger.cash(ledger.source, Currency::USD), dec!(874.63));
        assert_eq!(ledger.cash(destination, Currency::USD), dec!(125.37));
        assert_eq!(
            ledger.snapshot().transactions.len(),
            before.transactions.len() + 2
        );
    }
}

#[test]
fn distributions_conserve_cash_and_record_the_actual_allocations() {
    for (currency, remaining, profit, allocations) in [
        (
            Currency::USD,
            dec!(749.85),
            dec!(250.15),
            [dec!(100.06), dec!(75.045), dec!(50.03), dec!(25.015)],
        ),
        (
            Currency::EUR,
            dec!(1399.75),
            dec!(600.25),
            [dec!(240.10), dec!(180.075), dec!(120.05), dec!(60.025)],
        ),
        (
            Currency::BTC,
            dec!(3.49999995),
            dec!(0.00000005),
            [
                dec!(0.00000002),
                dec!(0.000000015),
                dec!(0.00000001),
                dec!(0.000000005),
            ],
        ),
    ] {
        let mut ledger = Ledger::new();
        ledger.configure_distribution(dec!(0));
        let before = ledger.snapshot();
        let result = ledger
            .trust
            .execute_distribution(ledger.source, profit, currency)
            .unwrap();
        assert_eq!(ledger.cash(ledger.source, currency), remaining);
        for (destination, expected) in ledger.children.into_iter().zip(allocations) {
            assert_eq!(ledger.cash(destination, currency), expected);
            let receipts = ledger.trust.get_account_transactions(destination).unwrap();
            assert_eq!(receipts.len(), 1);
            let receipt = &receipts[0];
            assert!(result.transactions_created.contains(&receipt.id));
            assert_eq!(
                (receipt.currency, receipt.category, receipt.amount),
                (currency, TransactionCategory::Deposit, expected)
            );
        }
        assert_eq!(result.transactions_created.len(), 4);
        let after = ledger.snapshot();
        assert_eq!(after.transactions.len(), before.transactions.len() + 8);
        assert_eq!(after.distributions.len(), 1);
        let history = &after.distributions[0];
        assert_eq!(history.source_account_id, ledger.source);
        assert_eq!(history.original_amount, profit);
        assert_eq!(
            [
                history.earnings_amount,
                history.tax_amount,
                history.reinvestment_amount,
                history.insurance_amount
            ],
            allocations.map(Some)
        );
        for other_currency in Currency::all()
            .into_iter()
            .filter(|other| *other != currency)
        {
            assert_eq!(
                after
                    .balances
                    .iter()
                    .filter(|balance| balance.currency == other_currency)
                    .collect::<Vec<_>>(),
                before
                    .balances
                    .iter()
                    .filter(|balance| balance.currency == other_currency)
                    .collect::<Vec<_>>()
            );
        }
        ledger.reopen();
        assert_eq!(ledger.snapshot(), after);
    }
}

#[test]
fn non_positive_cash_adjustments_and_distributions_never_write_receipts() {
    let mut ledger = Ledger::new();
    ledger.configure_distribution(dec!(0));
    let source = ledger
        .accounts()
        .into_iter()
        .find(|account| account.id == ledger.source)
        .unwrap();
    let before = ledger.snapshot();
    for amount in [dec!(0), dec!(-0.01), Decimal::MIN] {
        for category in [
            TransactionCategory::Deposit,
            TransactionCategory::Withdrawal,
        ] {
            assert!(ledger
                .trust
                .create_transaction(&source, &category, amount, &Currency::USD)
                .is_err());
            assert_eq!(ledger.snapshot(), before);
        }
        assert!(ledger
            .trust
            .execute_distribution(ledger.source, amount, Currency::USD)
            .is_err());
        assert_eq!(ledger.snapshot(), before);
    }
}

#[test]
fn distribution_threshold_and_available_cash_are_enforced_without_side_effects() {
    let mut ledger = Ledger::new();
    ledger.configure_distribution(dec!(100));
    let before = ledger.snapshot();
    for profit in [dec!(99.99), dec!(1000.01)] {
        assert!(
            ledger
                .trust
                .execute_distribution(ledger.source, profit, Currency::USD)
                .is_err(),
            "accepted ineligible distribution of {profit}"
        );
        assert_eq!(ledger.snapshot(), before);
    }
    // The minimum is inclusive; testing only rejection would allow a broken
    // implementation that rejects every request to pass.
    ledger
        .trust
        .execute_distribution(ledger.source, dec!(100), Currency::USD)
        .unwrap();
    assert_eq!(ledger.cash(ledger.source, Currency::USD), dec!(900));
    for (account, expected) in
        ledger
            .children
            .into_iter()
            .zip([dec!(40), dec!(30), dec!(20), dec!(10)])
    {
        assert_eq!(ledger.cash(account, Currency::USD), expected);
    }
    assert_eq!(ledger.snapshot().distributions.len(), 1);
}

#[test]
fn failed_distributions_roll_back_every_leg_and_history_before_retry() {
    for boundary in [
        "second allocation",
        "history",
        "source projection",
        "last destination projection",
    ] {
        let mut ledger = Ledger::new();
        ledger.configure_distribution(dec!(0));
        let before = ledger.snapshot();
        match boundary {
            "second allocation" => ledger.fail_writes(
                "INSERT",
                "transactions",
                &format!("WHEN NEW.account_id = '{}'", ledger.children[1]),
            ),
            "history" => ledger.fail_writes("INSERT", "distribution_history", ""),
            "source projection" => ledger.fail_writes("UPDATE", "accounts_balances", ""),
            _ => ledger.fail_writes(
                "INSERT",
                "accounts_balances",
                &format!("WHEN NEW.account_id = '{}'", ledger.children[3]),
            ),
        }
        let error = ledger
            .trust
            .execute_distribution(ledger.source, dec!(250.15), Currency::USD)
            .unwrap_err();
        assert!(
            error.to_string().contains("injected cash write failure"),
            "{boundary}: {error}"
        );
        assert_eq!(
            ledger.snapshot(),
            before,
            "partial distribution at {boundary}"
        );
        ledger.sql("DROP TRIGGER cash_failure");
        ledger.reopen();
        assert_eq!(ledger.snapshot(), before);
        ledger
            .trust
            .execute_distribution(ledger.source, dec!(250.15), Currency::USD)
            .unwrap();
        assert_eq!(ledger.cash(ledger.source, Currency::USD), dec!(749.85));
        for (destination, expected) in
            ledger
                .children
                .into_iter()
                .zip([dec!(100.06), dec!(75.045), dec!(50.03), dec!(25.015)])
        {
            assert_eq!(ledger.cash(destination, Currency::USD), expected);
        }
        let after = ledger.snapshot();
        assert_eq!(after.transactions.len(), before.transactions.len() + 8);
        assert_eq!(after.distributions.len(), 1);
    }
}

#[test]
fn arbitrary_transfer_sequences_cannot_create_money_or_overdraw() {
    use proptest::test_runner::{Config, TestRunner};
    let config = Config {
        cases: 32,
        source_file: Some(file!()),
        ..Config::default()
    };
    TestRunner::new(config)
        .run(
            &prop::collection::vec((0usize..3, 0usize..3, 0i64..150_001), 1..25),
            |operations| {
                let mut ledger = Ledger::new();
                let accounts = [ledger.source, ledger.children[0], ledger.children[1]];
                // Independent reference: 100,000 cents belong to the parent. A transfer
                // can only move those existing cents, never mint them or borrow euros.
                let mut cents = [100_000i64, 0, 0];
                for (from, to, amount) in operations {
                    let before = ledger.snapshot();
                    let result = ledger.trust.transfer_between_accounts(
                        accounts[from],
                        accounts[to],
                        Decimal::new(amount, 2),
                        Currency::USD,
                        "generated cash movement",
                    );
                    let permitted = from != to && amount > 0 && amount <= cents[from];
                    assert_eq!(result.is_ok(), permitted);
                    if permitted {
                        cents[from] -= amount;
                        cents[to] += amount;
                    } else {
                        assert_eq!(ledger.snapshot(), before);
                    }
                    for (account, expected) in accounts.into_iter().zip(cents) {
                        assert_eq!(
                            ledger.cash(account, Currency::USD),
                            Decimal::new(expected, 2)
                        );
                    }
                    assert_eq!(ledger.cash(ledger.source, Currency::EUR), dec!(2000));
                    assert_eq!(ledger.cash(ledger.source, Currency::BTC), dec!(3.5));
                }
                let persisted = ledger.snapshot();
                ledger.reopen();
                assert_eq!(ledger.snapshot(), persisted);
                Ok(())
            },
        )
        .unwrap();
}
