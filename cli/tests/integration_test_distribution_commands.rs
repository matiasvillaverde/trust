#![cfg(feature = "legacy-cli")]
//! Process-level contracts for the refactored distribution command group.
mod support;
use core::TrustFacade;
use db_sqlite::SqliteDatabase;
use model::{Account, AccountType, Currency, Environment, TransactionCategory};
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use std::process::Output;

const KEYWORD: &str = "I_UNDERSTAND_RISK";
const PASSWORD: &str = "fixture-distribution-password";

struct Fixture {
    database: support::TestDatabase,
    trust: TrustFacade,
    accounts: Vec<Account>,
}
impl Fixture {
    fn new() -> Self {
        let database = support::TestDatabase::new();
        let mut trust =
            TrustFacade::new_with_brokers(Box::new(SqliteDatabase::new(&database.url())), vec![]);
        let primary = trust
            .create_account("source", "", Environment::Paper, dec!(0), dec!(0))
            .unwrap();
        let mut accounts = vec![primary.clone()];
        for (name, kind) in [
            ("earnings", AccountType::Earnings),
            ("tax", AccountType::TaxReserve),
            ("reinvestment", AccountType::Reinvestment),
            ("insurance", AccountType::Insurance),
        ] {
            accounts.push(
                trust
                    .create_account_with_hierarchy(
                        name,
                        "",
                        Environment::Paper,
                        dec!(0),
                        dec!(0),
                        kind,
                        Some(primary.id),
                    )
                    .unwrap(),
            );
        }
        for account in &accounts {
            let amount = if account.id == primary.id {
                dec!(500)
            } else {
                dec!(1)
            };
            trust
                .create_transaction(
                    account,
                    &TransactionCategory::Deposit,
                    amount,
                    &Currency::USD,
                )
                .unwrap();
        }
        Self {
            database,
            trust,
            accounts,
        }
    }
    fn run(&self, args: &[String]) -> Output {
        support::legacy_command(&self.database.url(), KEYWORD)
            .args(args)
            .output()
            .unwrap()
    }
    fn configure(&self, field: Option<(&str, &str)>) -> Output {
        let mut arguments = vec![
            "distribution".into(),
            "configure".into(),
            format!("--account-id={}", self.accounts[0].id),
            format!("--password={PASSWORD}"),
            format!("--confirm-protected={KEYWORD}"),
        ];
        for (name, default) in [
            ("earnings", "40"),
            ("tax", "30"),
            ("reinvestment", "20"),
            ("insurance", "10"),
            ("threshold", "10"),
        ] {
            let value = field
                .filter(|(key, _)| *key == name)
                .map_or(default, |(_, value)| value);
            arguments.push(format!("--{name}={value}"));
        }
        self.run(&arguments)
    }
    fn execute(&self, account: &str, amount: &str) -> Output {
        self.run(&[
            "distribution".into(),
            "execute".into(),
            format!("--account-id={account}"),
            format!("--amount={amount}"),
            format!("--confirm-protected={KEYWORD}"),
        ])
    }
    fn balances(&mut self) -> Vec<Decimal> {
        self.accounts
            .iter()
            .map(|account| {
                self.trust
                    .search_balance(account.id, &Currency::USD)
                    .unwrap()
                    .total_available
            })
            .collect()
    }
}
fn success(output: Output) -> String {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap()
}
fn rejected(output: Output, code: &str) {
    assert!(!output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stderr).contains(code),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn distribution_commands_move_exact_cash_and_report_persisted_receipts() {
    let mut f = Fixture::new();
    success(f.configure(None));
    let account = f.accounts[0].id.to_string();
    success(f.execute(&account, "100"));
    assert_eq!(
        f.balances(),
        vec![dec!(400), dec!(41), dec!(31), dec!(21), dec!(11)]
    );
    let history = f.trust.distribution_history(f.accounts[0].id).unwrap();
    assert_eq!(history.len(), 1);
    assert_eq!(history[0].original_amount, dec!(100));
    assert_eq!(history[0].tax_amount, Some(dec!(30)));
    assert_eq!(history[0].insurance_amount, Some(dec!(10)));
    let output = success(f.run(&[
        "distribution".into(),
        "history".into(),
        format!("--account-id={account}"),
    ]));
    assert!(
        output.contains("amount=100")
            && output.contains("tax=30")
            && output.contains("insurance=10"),
        "{output}"
    );
    let output = success(f.run(&[
        "distribution".into(),
        "insurance".into(),
        format!("--account-id={account}"),
    ]));
    assert!(output.contains("10"), "{output}");
    assert_eq!(
        f.trust.distribution_history(f.accounts[0].id).unwrap(),
        history
    );
}

#[test]
fn unconfigured_account_cannot_distribute_or_use_another_accounts_reserve() {
    let mut f = Fixture::new();
    success(f.configure(None));
    let other = f
        .trust
        .create_account("unconfigured", "", Environment::Paper, dec!(0), dec!(0))
        .unwrap();
    f.trust
        .create_transaction(
            &other,
            &TransactionCategory::Deposit,
            dec!(500),
            &Currency::USD,
        )
        .unwrap();
    let account = other.id.to_string();
    let before = f.balances();
    rejected(
        f.run(&[
            "distribution".into(),
            "rules".into(),
            "show".into(),
            format!("--account-id={account}"),
        ]),
        "distribution_rules_failed",
    );
    rejected(
        f.run(&[
            "distribution".into(),
            "insurance".into(),
            format!("--account-id={account}"),
        ]),
        "insurance_account_not_found",
    );
    rejected(f.execute(&account, "100"), "execute_distribution_failed");
    assert_eq!(
        f.trust
            .search_balance(other.id, &Currency::USD)
            .unwrap()
            .total_available,
        dec!(500)
    );
    assert_eq!(f.balances(), before);
    assert!(f.trust.distribution_history(other.id).unwrap().is_empty());
    assert!(f
        .trust
        .distribution_history(f.accounts[0].id)
        .unwrap()
        .is_empty());
}

#[test]
fn invalid_distribution_commands_preserve_policy_cash_and_audit_history() {
    let mut f = Fixture::new();
    success(f.configure(None));
    let id = f.accounts[0].id;
    let account = id.to_string();
    let policy = f.trust.distribution_rules_for_account(id).unwrap();
    let balances = f.balances();
    let transactions = f.trust.get_account_transactions(id).unwrap();
    for field in ["earnings", "tax", "reinvestment", "insurance", "threshold"] {
        rejected(f.configure(Some((field, "NaN"))), "invalid_decimal");
    }
    rejected(
        f.configure(Some(("earnings", "-1"))),
        "configure_distribution_failed",
    );
    rejected(
        f.configure(Some(("earnings", "101"))),
        "configure_distribution_failed",
    );
    rejected(f.execute("invalid-uuid", "100"), "invalid_uuid");
    rejected(f.execute(&account, "NaN"), "invalid_decimal");
    for amount in ["-1", "0", "9.99", "500.01"] {
        rejected(f.execute(&account, amount), "execute_distribution_failed");
    }
    // Reopen after the rejected subprocesses: neither cached policy nor balances can mask writes.
    f.trust =
        TrustFacade::new_with_brokers(Box::new(SqliteDatabase::new(&f.database.url())), vec![]);
    assert_eq!(f.trust.distribution_rules_for_account(id).unwrap(), policy);
    assert_eq!(f.balances(), balances);
    assert_eq!(f.trust.get_account_transactions(id).unwrap(), transactions);
    assert!(f.trust.distribution_history(id).unwrap().is_empty());
}
