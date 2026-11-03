//! Offline query diagnostic. Index experiments exist only in this disposable DB.
use super::*;
use std::time::Instant;

const CASH_SQL: &str = "SELECT category, amount, trade_id FROM transactions
WHERE deleted_at IS NULL AND account_id = ? AND currency = ?
AND category IN (?, ?, ?, ?, ?, ?, ?, ?)
ORDER BY created_at, id";

#[test]
#[ignore = "Offline SQL diagnostic; run with --release --ignored --nocapture"]
fn profile_cash_ledger_access() {
    let (db, connection) = create_database_with_connection();
    let account = create_account(&db, "profile");
    let trade = create_persisted_trade(&db, &account, "PROFILE");
    let mut conn = connection.lock().unwrap();
    seed(&mut conn, &account, trade.id);
    let baseline =
        WorkerTransaction::read_cash_movements(&mut conn, account.id, &Currency::USD).unwrap();
    assert_eq!(baseline.len(), 14_401);
    let cash = baseline
        .iter()
        .fold(dec!(0), |cash, (category, amount)| match category {
            TransactionCategory::FundTrade(_) => cash.checked_sub(*amount).unwrap(),
            TransactionCategory::Deposit
            | TransactionCategory::PaymentFromTrade(_)
            | TransactionCategory::PaymentEarnings(_) => cash.checked_add(*amount).unwrap(),
            _ => unreachable!("Only the seeded cash categories may be returned"),
        });
    assert_eq!(cash, dec!(1_048_000));
    let before = measure(&mut conn, account.id, "baseline", &baseline);
    diesel::sql_query("CREATE INDEX profile_cash_cover ON transactions(account_id, currency, created_at, id, category, amount, trade_id) WHERE deleted_at IS NULL")
        .execute(&mut *conn).unwrap();
    let covering = measure(&mut conn, account.id, "covering_index", &baseline);
    diesel::sql_query("DROP INDEX profile_cash_cover")
        .execute(&mut *conn)
        .unwrap();
    diesel::sql_query("CREATE INDEX profile_cash_category_cover ON transactions(account_id, currency, category, created_at, id, amount, trade_id) WHERE deleted_at IS NULL")
        .execute(&mut *conn).unwrap();
    let category = measure(&mut conn, account.id, "category_covering_index", &baseline);
    if let Ok(path) = std::env::var("TRUST_PROFILE_LEDGER_OUTPUT") {
        std::fs::write(
            path,
            serde_json::to_vec_pretty(&[before, covering, category]).unwrap(),
        )
        .unwrap();
    }
}

fn seed(conn: &mut SqliteConnection, account: &Account, trade_id: Uuid) {
    use TransactionCategory::*;
    WorkerTransaction::create_transaction(
        conn,
        account.id,
        dec!(1_000_000),
        &Currency::USD,
        Deposit,
    )
    .unwrap();
    // Synthetic ledger-volume fixture, not a claim of broker lifecycle coverage.
    for _ in 0..4_800 {
        for (category, amount) in [
            (FundTrade(trade_id), dec!(100)),
            (OpenTrade(trade_id), dec!(100)),
            (CloseTarget(trade_id), dec!(110)),
            (PaymentFromTrade(trade_id), dec!(100)),
            (PaymentEarnings(trade_id), dec!(10)),
        ] {
            WorkerTransaction::create_transaction(
                conn,
                account.id,
                amount,
                &Currency::USD,
                category,
            )
            .unwrap();
        }
    }
}

fn measure(
    conn: &mut SqliteConnection,
    account: Uuid,
    name: &str,
    expected: &[(TransactionCategory, Decimal)],
) -> serde_json::Value {
    // Warm up after a schema change invalidates SQLite's prepared statements.
    assert_eq!(
        WorkerTransaction::read_cash_movements(conn, account, &Currency::USD).unwrap(),
        expected
    );
    let mut samples = Vec::new();
    for _ in 0..11 {
        let start = Instant::now();
        let actual = WorkerTransaction::read_cash_movements(conn, account, &Currency::USD).unwrap();
        samples.push(start.elapsed().as_micros());
        assert_eq!(
            actual, expected,
            "index choice must preserve exact, ordered cash inputs"
        );
    }
    samples.sort_unstable();
    let report = serde_json::json!({"case":name,"rows":expected.len(),"median_us":samples.get(5).unwrap(),"samples_us":samples,"plan":plan(conn, account)});
    println!("ledger_access {report}");
    report
}

fn plan(conn: &mut SqliteConnection, account: Uuid) -> Vec<String> {
    #[derive(QueryableByName)]
    struct Plan {
        #[diesel(sql_type = diesel::sql_types::Text)]
        detail: String,
    }
    let mut query = diesel::sql_query(format!("EXPLAIN QUERY PLAN {CASH_SQL}"))
        .bind::<diesel::sql_types::Text, _>(account.to_string())
        .bind::<diesel::sql_types::Text, _>("USD")
        .into_boxed();
    for category in super::super::super::CASH_FLOW_CATEGORIES {
        query = query.bind::<diesel::sql_types::Text, _>(category);
    }
    query
        .load::<Plan>(conn)
        .unwrap()
        .into_iter()
        .map(|row| row.detail)
        .collect()
}
