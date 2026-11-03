//! Cash saved during a write must remain exact through failures and recovery.
use super::*;
use diesel::sql_types::Text;
use std::sync::atomic::{AtomicUsize, Ordering};

fn funded_ledger() -> Ledger {
    let ledger = Ledger::new();
    ledger.add(
        TransactionCategory::Deposit,
        dec!(1000),
        "2026-11-01 00:00:00",
    );
    assert_eq!(ledger.cash(), dec!(1000));
    ledger
}

#[test]
fn appended_cash_uses_one_balance_lookup_and_one_schema_guard() {
    let ledger = funded_ledger();
    ledger.add(
        TransactionCategory::Deposit,
        dec!(25.125),
        "2026-11-02 00:00:00",
    );
    ledger.add(
        TransactionCategory::Withdrawal,
        dec!(70.00000001),
        "2026-11-02 01:00:00",
    );
    let statements = Arc::new(AtomicUsize::new(0));
    let observed = statements.clone();
    ledger.connection.lock().unwrap().set_instrumentation(
        move |event: diesel::connection::InstrumentationEvent<'_>| {
            if matches!(
                event,
                diesel::connection::InstrumentationEvent::FinishQuery { .. }
            ) {
                observed.fetch_add(1, Ordering::SeqCst);
            }
        },
    );
    for _ in 0..8 {
        assert_eq!(ledger.cash(), dec!(955.12499999));
    }
    assert_eq!(statements.load(Ordering::SeqCst), 16,
        "each verified read has one metadata guard and one balance lookup, with no ledger replay or writes");
}

#[derive(QueryableByName)]
struct TriggerSql {
    #[diesel(sql_type = Text)]
    sql: String,
}

#[test]
fn recreating_an_identical_monitor_cannot_hide_unprocessed_cash() {
    let ledger = funded_ledger();
    let definition = diesel::sql_query(
        "SELECT sql FROM temp.sqlite_schema WHERE name='trust_verified_cash_append'",
    )
    .get_result::<TriggerSql>(&mut *ledger.connection.lock().unwrap())
    .unwrap()
    .sql;
    ledger.sql("DROP TRIGGER temp.trust_verified_cash_append");
    ledger.add(
        TransactionCategory::Deposit,
        dec!(25.125),
        "2026-11-02 00:00:00",
    );
    ledger.sql(&definition.replacen("CREATE TRIGGER", "CREATE TEMP TRIGGER", 1));
    assert_eq!(
        ledger.cash(),
        dec!(1025.125),
        "the monitor was absent when this receipt was written"
    );
    ledger.add(
        TransactionCategory::Withdrawal,
        dec!(10),
        "2026-11-03 00:00:00",
    );
    assert_eq!(ledger.cash(), dec!(1015.125));
}

#[test]
fn restored_edit_monitors_cannot_hide_a_missed_update_or_deletion() {
    for (monitor, change, expected) in [
        (
            "trust_verified_cash_update",
            "UPDATE transactions SET amount='500'",
            dec!(500),
        ),
        (
            "trust_verified_cash_delete",
            "DELETE FROM transactions",
            dec!(0),
        ),
    ] {
        let ledger = funded_ledger();
        let definition = diesel::sql_query("SELECT sql FROM temp.sqlite_schema WHERE name = ?1")
            .bind::<Text, _>(monitor)
            .get_result::<TriggerSql>(&mut *ledger.connection.lock().unwrap())
            .unwrap()
            .sql;
        ledger.sql(&format!("DROP TRIGGER temp.{monitor}"));
        ledger.sql(change);
        ledger.sql(&definition.replacen("CREATE TRIGGER", "CREATE TEMP TRIGGER", 1));
        // No insert occurred, so the independent pending-write guard cannot
        // hide a broken schema epoch check in this experiment.
        assert_eq!(ledger.cash(), expected);
        ledger.add(
            TransactionCategory::Deposit,
            dec!(10),
            "2026-11-03 00:00:00",
        );
        assert_eq!(ledger.cash(), expected + dec!(10));
    }
}

#[test]
fn failure_to_save_verified_cash_rolls_back_the_receipt() {
    let ledger = funded_ledger();
    ledger.sql("CREATE TEMP TRIGGER reject_cash_update BEFORE UPDATE ON trust_verified_cash
      WHEN NEW.total != OLD.total BEGIN SELECT RAISE(ABORT, 'injected projection write failure'); END");
    assert_eq!(ledger.cash(), dec!(1000));
    let before = ledger
        .db
        .transaction_read()
        .all_transactions(ledger.account.id, &Currency::USD)
        .unwrap();
    let error = ledger
        .db
        .transaction_write()
        .create_transaction_by_account_id(
            ledger.account.id,
            dec!(25.125),
            &Currency::USD,
            TransactionCategory::Deposit,
        )
        .unwrap_err();
    assert!(
        error
            .to_string()
            .contains("injected projection write failure"),
        "{error}"
    );
    assert_eq!(
        ledger
            .db
            .transaction_read()
            .all_transactions(ledger.account.id, &Currency::USD)
            .unwrap(),
        before
    );
    assert_eq!(ledger.cash(), dec!(1000));
    ledger.sql("DROP TRIGGER temp.reject_cash_update");
    ledger
        .db
        .transaction_write()
        .create_transaction_by_account_id(
            ledger.account.id,
            dec!(25.125),
            &Currency::USD,
            TransactionCategory::Deposit,
        )
        .unwrap();
    assert_eq!(ledger.cash(), dec!(1025.125));
}

#[test]
fn an_overflowed_append_cannot_be_cured_by_a_later_offsetting_receipt() {
    let ledger = Ledger::new();
    ledger.add(
        TransactionCategory::Deposit,
        Decimal::MAX,
        "2026-11-01 00:00:00",
    );
    assert_eq!(ledger.cash(), Decimal::MAX);
    let invalid = ledger.add(TransactionCategory::Deposit, dec!(1), "2026-11-02 00:00:00");
    ledger.add(
        TransactionCategory::Withdrawal,
        dec!(1),
        "2026-11-03 00:00:00",
    );
    for _ in 0..2 {
        let error = ledger
            .db
            .transaction_read()
            .verified_cash_balance(ledger.account.id, &Currency::USD)
            .unwrap_err();
        assert!(
            error
                .to_string()
                .contains("Arithmetic overflow in addition"),
            "{error}"
        );
    }
    ledger.sql(&format!(
        "UPDATE transactions SET amount='0' WHERE id='{invalid}'"
    ));
    assert_eq!(ledger.cash(), Decimal::MAX - dec!(1));
}

#[test]
fn malformed_appended_receipts_invalidate_saved_cash_until_repaired() {
    for (column, value) in [
        ("amount", "'invalid'"),
        ("trade_id", "'invalid'"),
        ("created_at", "X'32303236'"),
        ("created_at", "CAST(X'ff' AS TEXT)"),
        ("id", "X'1234'"),
    ] {
        let ledger = funded_ledger();
        ledger.sql("PRAGMA foreign_keys=OFF");
        let id = Uuid::new_v4();
        let mut fields = std::collections::BTreeMap::from([
            ("id", format!("'{id}'")),
            ("created_at", "'2026-11-02 00:00:00'".into()),
            ("amount", "'25.125'".into()),
            ("trade_id", "NULL".into()),
        ]);
        fields.insert(column, value.into());
        ledger.sql(&format!(
            "INSERT INTO transactions
          (id, created_at, updated_at, account_id, currency, category, amount, trade_id)
          VALUES ({}, {}, '2026-11-02', '{}', 'USD', 'deposit', {}, {})",
            fields.get("id").unwrap(),
            fields.get("created_at").unwrap(),
            ledger.account.id,
            fields.get("amount").unwrap(),
            fields.get("trade_id").unwrap()
        ));
        for _ in 0..2 {
            let error = ledger
                .db
                .transaction_read()
                .verified_cash_balance(ledger.account.id, &Currency::USD)
                .unwrap_err();
            let expected = match column {
                "amount" => "Invalid decimal",
                "trade_id" => "invalid",
                _ => "Cash ledger ordering key",
            };
            let mut cause = Some(error.as_ref());
            let mut observed = false;
            while let Some(error) = cause {
                observed |= error.to_string().contains(expected);
                cause = error.source();
            }
            assert!(observed, "{column}: {error:?}");
        }
        ledger.sql("DELETE FROM transactions WHERE created_at != '2026-11-01 00:00:00'");
        assert_eq!(ledger.cash(), dec!(1000));
    }
}

#[test]
fn other_trigger_effects_are_included_before_cash_can_be_reused() {
    let ledger = funded_ledger();
    ledger.sql(
        "CREATE TEMP TRIGGER adjust_receipt AFTER INSERT ON main.transactions
      WHEN NEW.amount='2' BEGIN UPDATE transactions SET amount='50' WHERE id=NEW.id; END",
    );
    assert_eq!(ledger.cash(), dec!(1000));
    ledger.add(TransactionCategory::Deposit, dec!(2), "2026-11-02 00:00:00");
    assert_eq!(ledger.cash(), dec!(1050));
}

#[test]
fn an_ignored_projection_completion_cannot_authorize_old_cash() {
    let ledger = funded_ledger();
    ledger.sql(
        "CREATE TEMP TRIGGER skip_completion BEFORE UPDATE ON trust_verified_cash
      WHEN OLD.pending_id IS NOT NULL AND NEW.pending_id IS NULL
      BEGIN SELECT RAISE(IGNORE); END",
    );
    assert_eq!(ledger.cash(), dec!(1000));
    ledger.add(
        TransactionCategory::Withdrawal,
        dec!(25.125),
        "2026-11-02 00:00:00",
    );
    assert_eq!(ledger.cash(), dec!(974.875));
    assert_eq!(ledger.cash(), dec!(974.875));
    ledger.add(
        TransactionCategory::Withdrawal,
        dec!(10),
        "2026-11-03 00:00:00",
    );
    assert_eq!(ledger.cash(), dec!(964.875));
    ledger.sql("DROP TRIGGER temp.skip_completion");
    assert_eq!(ledger.cash(), dec!(964.875));
}

#[test]
fn a_commit_between_metadata_and_cash_lookup_cannot_reuse_old_money() {
    use std::sync::atomic::AtomicBool;
    let ledger = funded_ledger();
    let mut external = ledger.external();
    let changed = Arc::new(AtomicBool::new(false));
    let observed = changed.clone();
    ledger.connection.lock().unwrap().set_instrumentation(
        move |event: diesel::connection::InstrumentationEvent<'_>| {
            if let diesel::connection::InstrumentationEvent::FinishQuery { query, .. } = event {
                if query.to_string().starts_with("PRAGMA temp.schema_version")
                    && !observed.swap(true, Ordering::SeqCst)
                {
                    external
                        .batch_execute("UPDATE transactions SET amount='500'")
                        .unwrap();
                }
            }
        },
    );
    assert_eq!(ledger.cash(), dec!(500));
    assert!(
        changed.load(Ordering::SeqCst),
        "the competing write must actually commit"
    );
    assert_eq!(ledger.cash(), dec!(500));
}

#[test]
fn restoring_a_backup_discards_cash_from_the_replaced_ledger() {
    let mut ledger = funded_ledger();
    let backup = ledger.directory.path().join("financial-backup.json");
    ledger.db.export_backup_to_path(&backup).unwrap();
    ledger.add(
        TransactionCategory::Deposit,
        dec!(25.125),
        "2026-11-02 00:00:00",
    );
    assert_eq!(ledger.cash(), dec!(1025.125));
    ledger
        .db
        .import_backup_from_path(
            &backup,
            crate::ImportOptions {
                mode: crate::ImportMode::Replace,
                dry_run: false,
            },
        )
        .unwrap();
    assert_eq!(ledger.cash(), dec!(1000));
    ledger.add(
        TransactionCategory::Withdrawal,
        dec!(10),
        "2026-11-03 00:00:00",
    );
    assert_eq!(ledger.cash(), dec!(990));
}

#[test]
fn abrupt_exit_discards_uncommitted_cash_and_reverifies_on_restart() {
    const CHILD_PATH: &str = "TRUST_CASH_CRASH_FIXTURE";
    const CHILD_ACCOUNT: &str = "TRUST_CASH_CRASH_ACCOUNT";
    if let Ok(path) = std::env::var(CHILD_PATH) {
        let account = Uuid::parse_str(&std::env::var(CHILD_ACCOUNT).unwrap()).unwrap();
        let mut db = SqliteDatabase::new(&path);
        assert_eq!(
            db.transaction_read()
                .verified_cash_balance(account, &Currency::USD)
                .unwrap(),
            dec!(1000)
        );
        db.begin_savepoint("uncommitted_cash").unwrap();
        db.transaction_write()
            .create_transaction_by_account_id(
                account,
                dec!(25.125),
                &Currency::USD,
                TransactionCategory::Deposit,
            )
            .unwrap();
        assert_eq!(
            db.transaction_read()
                .verified_cash_balance(account, &Currency::USD)
                .unwrap(),
            dec!(1025.125)
        );
        // Deliberately bypass Rust destructors and all application rollback code.
        std::process::exit(47);
    }
    let ledger = funded_ledger();
    let path = ledger.directory.path().join("ledger.db");
    let child = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "workers::worker_transaction::tests::ledger::checkpoints::write_through::abrupt_exit_discards_uncommitted_cash_and_reverifies_on_restart", "--test-threads=1"])
        .env(CHILD_PATH, &path).env(CHILD_ACCOUNT, ledger.account.id.to_string()).output().unwrap();
    assert_eq!(
        child.status.code(),
        Some(47),
        "{}",
        String::from_utf8_lossy(&child.stderr)
    );
    let reopened = SqliteDatabase::new(path.to_str().unwrap());
    assert_eq!(
        reopened
            .transaction_read()
            .verified_cash_balance(ledger.account.id, &Currency::USD)
            .unwrap(),
        dec!(1000)
    );
    assert_eq!(ledger.cash(), dec!(1000));
    assert_eq!(
        reopened
            .transaction_read()
            .all_transactions(ledger.account.id, &Currency::USD)
            .unwrap()
            .len(),
        1
    );
}
