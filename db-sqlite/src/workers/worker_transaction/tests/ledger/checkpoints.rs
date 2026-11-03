//! Verify money and visibility across real SQLite connections/savepoints. The
//! expected totals are independent of the checkpoint and ledger-fold code.
use super::*;
use diesel::connection::SimpleConnection;

mod write_through;

struct Ledger {
    directory: tempfile::TempDir,
    db: SqliteDatabase,
    connection: Arc<Mutex<SqliteConnection>>,
    account: Account,
}

impl Ledger {
    fn new() -> Self {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("ledger.db");
        let initialized = SqliteDatabase::new(path.to_str().unwrap());
        let account = create_account(&initialized, "checkpoints");
        drop(initialized);
        let mut connection = SqliteConnection::establish(path.to_str().unwrap()).unwrap();
        connection
            .batch_execute(
                "PRAGMA journal_mode=WAL; PRAGMA temp_store=MEMORY; PRAGMA foreign_keys=ON;",
            )
            .unwrap();
        let connection = Arc::new(Mutex::new(connection));
        Self {
            directory,
            db: SqliteDatabase::new_from(connection.clone()),
            connection,
            account,
        }
    }
    fn external(&self) -> SqliteConnection {
        SqliteConnection::establish(self.directory.path().join("ledger.db").to_str().unwrap())
            .unwrap()
    }
    fn add(&self, category: TransactionCategory, amount: Decimal, date: &str) -> Uuid {
        insert_transaction_row(
            &mut self.connection.lock().unwrap(),
            self.account.id,
            category,
            Currency::USD,
            amount,
            super::date(date),
            None,
        )
    }
    fn cash(&self) -> Decimal {
        self.db
            .transaction_read()
            .verified_cash_balance(self.account.id, &Currency::USD)
            .unwrap()
    }
    fn sql(&self, sql: &str) {
        self.connection.lock().unwrap().batch_execute(sql).unwrap();
    }
}

#[test]
fn cash_reads_rebind_account_currency_and_preserve_fractional_appends() {
    let ledger = Ledger::new();
    let other = create_account(&ledger.db, "other");
    let trade = create_persisted_trade(&ledger.db, &ledger.account, "CHECKPOINT");
    ledger.add(
        TransactionCategory::Deposit,
        dec!(1000.00000001),
        "2026-11-01 00:00:00",
    );
    assert_eq!(ledger.cash(), dec!(1000.00000001));
    for (account, currency, amount) in [
        (other.id, Currency::USD, dec!(999)),
        (ledger.account.id, Currency::EUR, dec!(888)),
    ] {
        ledger
            .db
            .transaction_write()
            .create_transaction_by_account_id(
                account,
                amount,
                &currency,
                TransactionCategory::Deposit,
            )
            .unwrap();
        assert_eq!(
            ledger
                .db
                .transaction_read()
                .verified_cash_balance(account, &currency)
                .unwrap(),
            amount
        );
        assert_eq!(ledger.cash(), dec!(1000.00000001));
    }
    for (category, amount, expected) in cash_flow_expectations(trade.id) {
        ledger
            .db
            .transaction_write()
            .create_transaction_by_account_id(ledger.account.id, amount, &Currency::USD, category)
            .unwrap();
        assert_eq!(ledger.cash(), expected);
        assert_eq!(
            ledger.cash(),
            expected,
            "an unchanged read preserves exact cash"
        );
    }
}

fn cash_flow_expectations(trade_id: Uuid) -> [(TransactionCategory, Decimal, Decimal); 8] {
    [
        (
            TransactionCategory::FundTrade(trade_id),
            dec!(100),
            dec!(900.00000001),
        ),
        (
            TransactionCategory::FeeOpen(trade_id),
            dec!(0.00000001),
            dec!(900),
        ),
        (
            TransactionCategory::PaymentFromTrade(trade_id),
            dec!(100),
            dec!(1000),
        ),
        (
            TransactionCategory::FeeClose(trade_id),
            dec!(1.25),
            dec!(998.75),
        ),
        (
            TransactionCategory::PaymentEarnings(trade_id),
            dec!(30),
            dec!(1028.75),
        ),
        (
            TransactionCategory::WithdrawalEarnings,
            dec!(28.75),
            dec!(1000),
        ),
        (TransactionCategory::Withdrawal, dec!(10), dec!(990)),
        (
            TransactionCategory::PaymentTax(trade_id),
            dec!(999),
            dec!(990),
        ),
    ]
}

#[test]
fn historical_edits_deletion_scope_moves_and_corrupt_receipts_invalidate_cash() {
    let ledger = Ledger::new();
    let other = create_account(&ledger.db, "moved");
    let receipt = ledger.add(
        TransactionCategory::Deposit,
        dec!(1000),
        "2026-11-01 00:00:00",
    );
    assert_eq!(ledger.cash(), dec!(1000));
    for (change, expected) in [
        ("amount='125.75'".to_owned(), dec!(125.75)),
        ("currency='EUR'".to_owned(), dec!(0)),
        ("currency='USD'".to_owned(), dec!(125.75)),
        (format!("account_id='{}'", other.id), dec!(0)),
        (format!("account_id='{}'", ledger.account.id), dec!(125.75)),
        ("deleted_at='2026-11-02 00:00:00'".to_owned(), dec!(0)),
        ("deleted_at=NULL".to_owned(), dec!(125.75)),
        ("category='withdrawal'".to_owned(), dec!(-125.75)),
    ] {
        ledger.sql(&format!(
            "UPDATE transactions SET {change} WHERE id='{}'",
            receipt
        ));
        assert_eq!(ledger.cash(), expected);
    }
    for corrupt in ["invalid", "0.123456789012345678901234567891"] {
        ledger.sql(&format!(
            "UPDATE transactions SET amount='{corrupt}' WHERE id='{}'",
            receipt
        ));
        assert!(ledger
            .db
            .transaction_read()
            .verified_cash_balance(ledger.account.id, &Currency::USD)
            .is_err());
    }
    ledger.sql(&format!(
        "UPDATE transactions SET amount='125.75' WHERE id='{}'",
        receipt
    ));
    assert_eq!(ledger.cash(), dec!(-125.75));
    ledger.sql(&format!("DELETE FROM transactions WHERE id='{}'", receipt));
    assert_eq!(ledger.cash(), dec!(0));
}

#[test]
fn nested_rollbacks_restore_cash_and_do_not_reuse_abandoned_prefixes() {
    let ledger = Ledger::new();
    ledger.add(
        TransactionCategory::Deposit,
        dec!(1000),
        "2026-11-01 00:00:00",
    );
    ledger.sql("SAVEPOINT outer_operation");
    assert_eq!(
        ledger.cash(),
        dec!(1000),
        "checkpoint installation is inside the savepoint"
    );
    ledger.add(
        TransactionCategory::Withdrawal,
        dec!(30),
        "2026-11-02 00:00:00",
    );
    assert_eq!(ledger.cash(), dec!(970));
    ledger.sql("SAVEPOINT inner_operation");
    ledger.add(
        TransactionCategory::Deposit,
        dec!(50),
        "2026-11-03 00:00:00",
    );
    assert_eq!(ledger.cash(), dec!(1020));
    ledger.sql("ROLLBACK TO inner_operation; RELEASE inner_operation");
    assert_eq!(ledger.cash(), dec!(970));
    ledger.sql("ROLLBACK TO outer_operation; RELEASE outer_operation");
    assert_eq!(ledger.cash(), dec!(1000));
    ledger.add(
        TransactionCategory::Withdrawal,
        dec!(75),
        "2026-11-03 00:00:00",
    );
    assert_eq!(ledger.cash(), dec!(925));
    ledger.sql("BEGIN");
    ledger.add(
        TransactionCategory::Deposit,
        dec!(100),
        "2026-11-04 00:00:00",
    );
    assert_eq!(ledger.cash(), dec!(1025));
    ledger.sql("ROLLBACK");
    assert_eq!(ledger.cash(), dec!(925));
}

#[test]
fn external_commits_and_reader_snapshots_never_publish_stale_cash() {
    let ledger = Ledger::new();
    let receipt = ledger.add(
        TransactionCategory::Deposit,
        dec!(1000),
        "2026-11-01 00:00:00",
    );
    assert_eq!(ledger.cash(), dec!(1000));
    ledger.sql("BEGIN; SELECT id FROM transactions LIMIT 1;");
    let mut external = ledger.external();
    external
        .batch_execute(&format!(
            "UPDATE transactions SET amount='500' WHERE id='{}'",
            receipt
        ))
        .unwrap();
    assert_eq!(
        ledger.cash(),
        dec!(1000),
        "the open WAL snapshot still contains the old ledger"
    );
    ledger.sql("COMMIT");
    assert_eq!(
        ledger.cash(),
        dec!(500),
        "the next snapshot must invalidate the old prefix"
    );
    external
        .batch_execute(&format!(
            "BEGIN; UPDATE transactions SET amount='invalid' WHERE id='{}'",
            receipt
        ))
        .unwrap();
    assert_eq!(
        ledger.cash(),
        dec!(500),
        "uncommitted external writes cannot affect risk capacity"
    );
    external.batch_execute("COMMIT").unwrap();
    assert!(ledger
        .db
        .transaction_read()
        .verified_cash_balance(ledger.account.id, &Currency::USD)
        .is_err());
    external
        .batch_execute(&format!(
            "UPDATE transactions SET amount='250.125' WHERE id='{}'",
            receipt
        ))
        .unwrap();
    assert_eq!(ledger.cash(), dec!(250.125));
    let reopened = SqliteDatabase::new(ledger.directory.path().join("ledger.db").to_str().unwrap());
    assert_eq!(
        reopened
            .transaction_read()
            .verified_cash_balance(ledger.account.id, &Currency::USD)
            .unwrap(),
        dec!(250.125)
    );
}

#[test]
fn backdated_cash_preserves_ordered_overflow_rejection() {
    let ledger = Ledger::new();
    ledger.add(
        TransactionCategory::Deposit,
        Decimal::MAX,
        "2026-11-01 00:00:00",
    );
    ledger.add(
        TransactionCategory::Withdrawal,
        dec!(10),
        "2026-11-03 00:00:00",
    );
    assert_eq!(ledger.cash(), Decimal::MAX.checked_sub(dec!(10)).unwrap());
    let historical = ledger.add(TransactionCategory::Deposit, dec!(1), "2026-11-02 00:00:00");
    let error = ledger
        .db
        .transaction_read()
        .verified_cash_balance(ledger.account.id, &Currency::USD)
        .unwrap_err();
    assert!(error
        .to_string()
        .contains("Arithmetic overflow in addition"));
    ledger.sql(&format!(
        "DELETE FROM transactions WHERE id='{}'",
        historical
    ));
    assert_eq!(ledger.cash(), Decimal::MAX.checked_sub(dec!(10)).unwrap());
}

#[test]
fn missing_or_replaced_monitor_triggers_force_reverification() {
    let ledger = Ledger::new();
    let receipt = ledger.add(
        TransactionCategory::Deposit,
        dec!(1000),
        "2026-11-01 00:00:00",
    );
    assert_eq!(ledger.cash(), dec!(1000));
    ledger.sql(&format!("DROP TRIGGER temp.trust_verified_cash_update; UPDATE transactions SET amount='500' WHERE id='{}'", receipt));
    assert_eq!(ledger.cash(), dec!(500));
    ledger.sql(&format!("DROP TRIGGER temp.trust_verified_cash_update;
      CREATE TEMP TRIGGER trust_verified_cash_update BEFORE UPDATE ON main.transactions BEGIN SELECT 1; END;
      UPDATE transactions SET amount='250' WHERE id='{}'", receipt));
    assert_eq!(ledger.cash(), dec!(250));
}

#[test]
fn replace_by_identity_or_rowid_cannot_double_count_prior_cash() {
    let ledger = Ledger::new();
    let receipt = ledger.add(
        TransactionCategory::Deposit,
        dec!(1000),
        "2026-11-01 00:00:00",
    );
    assert_eq!(ledger.cash(), dec!(1000));
    // Recursive delete triggers are deliberately disabled: BEFORE INSERT must
    // detect REPLACE conflicts even when SQLite omits the implicit delete event.
    ledger.sql("PRAGMA recursive_triggers=OFF");
    ledger.sql(&format!(
        "INSERT OR REPLACE INTO transactions
      (id, created_at, updated_at, account_id, currency, category, amount)
      VALUES ('{receipt}', '2026-11-03', '2026-11-03', '{}', 'USD', 'deposit', '500')",
        ledger.account.id
    ));
    assert_eq!(ledger.cash(), dec!(500));
    let replacement = Uuid::new_v4();
    ledger.sql(&format!("INSERT OR REPLACE INTO transactions
      (rowid, id, created_at, updated_at, account_id, currency, category, amount)
      SELECT rowid, '{replacement}', '2026-11-04', '2026-11-04', account_id, currency, category, '250'
      FROM transactions WHERE id='{receipt}'"));
    assert_eq!(ledger.cash(), dec!(250));
    let other = ledger.add(
        TransactionCategory::Deposit,
        dec!(50),
        "2026-11-05 00:00:00",
    );
    assert_eq!(ledger.cash(), dec!(300));
    ledger.sql(&format!(
        "UPDATE OR REPLACE transactions SET id='{replacement}', amount='125' WHERE id='{other}'"
    ));
    assert_eq!(ledger.cash(), dec!(125));
}

#[test]
fn same_timestamp_earlier_ids_replay_before_the_saved_boundary() {
    let ledger = Ledger::new();
    let receipt = ledger.add(
        TransactionCategory::Deposit,
        dec!(1000),
        "2026-11-01 00:00:00",
    );
    ledger.sql(&format!(
        "UPDATE transactions SET id='ffffffff-ffff-ffff-ffff-ffffffffffff' WHERE id='{receipt}'"
    ));
    assert_eq!(ledger.cash(), dec!(1000));
    ledger.sql(&format!("INSERT INTO transactions
      (id, created_at, updated_at, account_id, currency, category, amount)
      VALUES ('00000000-0000-0000-0000-000000000001', '2026-11-01 00:00:00', '2026-11-01 00:00:00', '{}', 'USD', 'deposit', '25.25')", ledger.account.id));
    assert_eq!(ledger.cash(), dec!(1025.25));
}

#[test]
fn malformed_trade_identity_cannot_hide_behind_an_existing_cash_checkpoint() {
    let ledger = Ledger::new();
    let trade = create_persisted_trade(&ledger.db, &ledger.account, "CORRUPT-CHECKPOINT");
    ledger.add(
        TransactionCategory::Deposit,
        dec!(1000),
        "2026-11-01 00:00:00",
    );
    let funding = ledger.add(
        TransactionCategory::FundTrade(trade.id),
        dec!(100),
        "2026-11-02 00:00:00",
    );
    assert_eq!(ledger.cash(), dec!(900));
    ledger.sql("PRAGMA foreign_keys=OFF");
    for identity in ["'invalid-uuid'", "NULL"] {
        ledger.sql(&format!(
            "UPDATE transactions SET trade_id={identity} WHERE id='{funding}'"
        ));
        assert!(ledger
            .db
            .transaction_read()
            .verified_cash_balance(ledger.account.id, &Currency::USD)
            .is_err());
    }
    ledger.sql(&format!(
        "UPDATE transactions SET trade_id='{}' WHERE id='{funding}'",
        trade.id
    ));
    assert_eq!(ledger.cash(), dec!(900));
}

#[test]
fn read_only_or_file_backed_temp_storage_uses_exact_full_replay() {
    let ledger = Ledger::new();
    let receipt = ledger.add(
        TransactionCategory::Deposit,
        dec!(125.75),
        "2026-11-01 00:00:00",
    );
    assert_eq!(ledger.cash(), dec!(125.75));
    for setting in [
        "PRAGMA temp_store=FILE",
        "PRAGMA temp_store=MEMORY; PRAGMA query_only=ON",
    ] {
        ledger.sql(setting);
        assert_eq!(ledger.cash(), dec!(125.75));
    }
    ledger.sql("PRAGMA query_only=OFF");
    ledger.sql(&format!(
        "UPDATE transactions SET amount='25.125' WHERE id='{receipt}'"
    ));
    assert_eq!(ledger.cash(), dec!(25.125));
}

#[test]
fn monthly_reference_query_branches_share_one_snapshot() {
    use std::sync::atomic::{AtomicBool, Ordering};
    let ledger = Ledger::new();
    let trade = create_persisted_trade(&ledger.db, &ledger.account, "MONTH-SNAPSHOT");
    let funding = ledger.add(
        TransactionCategory::FundTrade(trade.id),
        dec!(1000),
        "2026-10-31 00:00:00",
    );
    let deposit = ledger.add(
        TransactionCategory::Deposit,
        dec!(500),
        "2026-11-01 00:00:00",
    );
    let changed = Arc::new(AtomicBool::new(false));
    let observed = changed.clone();
    let mut external = ledger.external();
    ledger.connection.lock().unwrap().set_instrumentation(
        move |event: diesel::connection::InstrumentationEvent<'_>| {
            if let diesel::connection::InstrumentationEvent::FinishQuery { query, .. } = event {
                // Test-only scheduling at the real storage boundary. Do not log SQL
                // arguments: the callback only recognizes the first completed SELECT.
                if query.to_string().starts_with("SELECT") && !observed.swap(true, Ordering::SeqCst)
                {
                    external
                        .batch_execute(&format!(
                            "BEGIN;
                  UPDATE transactions SET amount='2000' WHERE id='{funding}';
                  UPDATE transactions SET amount='999' WHERE id='{deposit}'; COMMIT;"
                        ))
                        .unwrap();
                }
            }
        },
    );
    let read = || {
        ledger
            .db
            .transaction_read()
            .monthly_reference_inputs(
                ledger.account.id,
                &Currency::USD,
                super::date("2026-11-01 00:00:00"),
            )
            .unwrap()
            .into_iter()
            .map(|tx| (tx.category, tx.amount))
            .collect::<Vec<_>>()
    };
    assert_eq!(
        read(),
        vec![
            (TransactionCategory::FundTrade(trade.id), dec!(1000)),
            (TransactionCategory::Deposit, dec!(500))
        ]
    );
    assert!(
        changed.load(Ordering::SeqCst),
        "the writer must actually commit between query branches"
    );
    assert_eq!(
        read(),
        vec![
            (TransactionCategory::FundTrade(trade.id), dec!(2000)),
            (TransactionCategory::Deposit, dec!(999))
        ]
    );
}

#[test]
fn malformed_cursor_storage_cannot_repeat_or_skip_cash_receipts() {
    for change in [
        "created_at = X'32303236'",
        "created_at = CAST(X'ff' AS TEXT)",
        "id = X'1234'",
        "id = CAST(X'ff' AS TEXT)",
    ] {
        let ledger = Ledger::new();
        let receipt = ledger.add(
            TransactionCategory::Deposit,
            dec!(1000),
            "2026-11-01 00:00:00",
        );
        assert_eq!(ledger.cash(), dec!(1000));
        ledger.sql(&format!(
            "UPDATE transactions SET {change} WHERE id='{receipt}'"
        ));
        let error = ledger
            .db
            .transaction_read()
            .verified_cash_balance(ledger.account.id, &Currency::USD)
            .unwrap_err();
        let mut cause = Some(error.as_ref());
        let mut ordering_error = false;
        while let Some(error) = cause {
            ordering_error |= error.to_string().contains("Cash ledger ordering key");
            cause = error.source();
        }
        assert!(ordering_error, "{error:?}");
    }
}

#[test]
fn failed_suffix_replay_remains_rejected_until_the_ledger_is_repaired() {
    let ledger = Ledger::new();
    ledger.add(
        TransactionCategory::Deposit,
        dec!(1000),
        "2026-11-01 00:00:00",
    );
    assert_eq!(ledger.cash(), dec!(1000));
    ledger.add(
        TransactionCategory::Deposit,
        dec!(25.125),
        "2026-11-02 00:00:00",
    );
    let invalid = Uuid::new_v4();
    ledger.sql(&format!(
        "INSERT INTO transactions
      (id, created_at, updated_at, account_id, currency, category, amount)
      VALUES ('{invalid}', '2026-11-03', '2026-11-03', '{}', 'USD', 'deposit', 'invalid')",
        ledger.account.id
    ));
    for _ in 0..2 {
        let error = ledger
            .db
            .transaction_read()
            .verified_cash_balance(ledger.account.id, &Currency::USD)
            .unwrap_err();
        assert!(
            error.downcast_ref::<rust_decimal::Error>().is_some(),
            "{error:?}"
        );
    }
    ledger.sql(&format!(
        "UPDATE transactions SET amount='10' WHERE id='{invalid}'"
    ));
    assert_eq!(ledger.cash(), dec!(1035.125));
    assert_eq!(ledger.cash(), dec!(1035.125));
}
