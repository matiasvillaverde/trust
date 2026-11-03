use super::*;
use chrono::NaiveDate;

mod checkpoints;
mod profiling;

fn date(value: &str) -> NaiveDateTime {
    NaiveDateTime::parse_from_str(value, "%Y-%m-%d %H:%M:%S").unwrap()
}
fn record(
    connection: &mut SqliteConnection,
    account: &Account,
    currency: Currency,
    category: TransactionCategory,
    amount: Decimal,
    created: NaiveDateTime,
) -> Transaction {
    let mut tx =
        WorkerTransaction::create_transaction(connection, account.id, amount, &currency, category)
            .unwrap();
    diesel::update(transactions::table.filter(transactions::id.eq(tx.id.to_string())))
        .set((
            transactions::created_at.eq(created),
            transactions::updated_at.eq(created),
        ))
        .execute(connection)
        .unwrap();
    tx.created_at = created;
    tx.updated_at = created;
    tx
}

#[test]
fn monthly_reference_preserves_flows_and_carry_receipts_without_crossing_scopes() {
    use TransactionCategory::*;
    let (db, connection) = create_database_with_connection();
    let owner = create_account(&db, "reference");
    let other = create_account(&db, "other");
    let carried = create_persisted_trade(&db, &owner, "CARRY");
    let recent = create_persisted_trade(&db, &owner, "RECENT");
    let deleted = create_persisted_trade(&db, &owner, "DELETED");
    let foreign = create_persisted_trade(&db, &other, "FOREIGN");
    let before = date("2026-10-31 23:59:59");
    let cutoff = date("2026-11-01 00:00:00");
    let mut conn = connection.lock().unwrap();
    let mut expected: Vec<_> = [
        (FundTrade(carried.id), dec!(1000), before),
        (PaymentFromTrade(carried.id), dec!(25), before),
        (Deposit, dec!(500), cutoff),
        (Withdrawal, dec!(50), cutoff),
        (PaymentEarnings(carried.id), dec!(75), cutoff),
        (WithdrawalEarnings, dec!(10), cutoff),
        (CloseTarget(carried.id), dec!(1110), cutoff),
        (CloseSafetyStop(carried.id), dec!(900), cutoff),
        (CloseSafetyStopSlippage(carried.id), dec!(880), cutoff),
    ]
    .into_iter()
    .map(|(category, amount, when)| {
        record(&mut conn, &owner, Currency::USD, category, amount, when)
    })
    .collect();
    seed_excluded_receipts(
        &mut conn,
        (&owner, &other),
        [carried.id, recent.id, deleted.id, foreign.id],
        before,
        cutoff,
    );
    let queries = capture_query_plans(&mut conn);
    let actual = WorkerTransaction::read_monthly_reference_inputs(
        &mut conn,
        owner.id,
        &Currency::USD,
        cutoff,
    )
    .unwrap();
    expected.sort_by_key(|tx| (tx.created_at, tx.id));
    assert_eq!(
        actual, expected,
        "every required receipt keeps its exact amount and identity"
    );
    explain_plan(&mut conn, queries);
}

#[test]
fn corrupt_required_reference_receipts_return_errors_instead_of_missing_capital() {
    let (db, connection) = create_database_with_connection();
    let account = create_account(&db, "corrupt-reference");
    let cutoff = NaiveDate::from_ymd_opt(2026, 11, 1)
        .unwrap()
        .and_hms_opt(0, 0, 0)
        .unwrap();
    let mut conn = connection.lock().unwrap();
    let tx = record(
        &mut conn,
        &account,
        Currency::USD,
        TransactionCategory::Deposit,
        dec!(125.75),
        cutoff,
    );
    let read = |conn: &mut SqliteConnection| {
        WorkerTransaction::read_monthly_reference_inputs(conn, account.id, &Currency::USD, cutoff)
    };
    assert_eq!(read(&mut conn).unwrap(), vec![tx.clone()]);
    diesel::update(transactions::table.filter(transactions::id.eq(tx.id.to_string())))
        .set(transactions::amount.eq("invalid-decimal"))
        .execute(&mut *conn)
        .unwrap();
    assert!(read(&mut conn).unwrap_err().to_string().contains("amount"));
    diesel::update(transactions::table.filter(transactions::id.eq(tx.id.to_string())))
        .set(transactions::amount.eq("125.75"))
        .execute(&mut *conn)
        .unwrap();
    assert_eq!(read(&mut conn).unwrap(), vec![tx]);
}

fn capture_query_plans(conn: &mut SqliteConnection) -> Arc<Mutex<Vec<String>>> {
    let queries = Arc::new(Mutex::new(Vec::new()));
    if std::env::var_os("TRUST_PROFILE_SQL_PLAN").is_some() {
        let captured = queries.clone();
        conn.set_instrumentation(move |event: diesel::connection::InstrumentationEvent<'_>| {
            // CacheQuery contains SQL placeholders, never bound values.
            if let diesel::connection::InstrumentationEvent::CacheQuery { sql, .. } = event {
                captured.lock().unwrap().push(sql.to_owned());
            }
        });
    }
    queries
}

fn explain_plan(conn: &mut SqliteConnection, queries: Arc<Mutex<Vec<String>>>) {
    if std::env::var_os("TRUST_PROFILE_SQL_PLAN").is_some() {
        conn.set_instrumentation(None::<fn(diesel::connection::InstrumentationEvent<'_>)>);
        #[derive(QueryableByName)]
        struct Plan {
            #[diesel(sql_type = diesel::sql_types::Text)]
            detail: String,
        }
        for query in queries.lock().unwrap().iter() {
            let mut plan = diesel::sql_query(format!("EXPLAIN QUERY PLAN {query}")).into_boxed();
            // Structural plans for the actual fixed SQL, with no financial data.
            for _ in query.matches('?') {
                plan = plan.bind::<diesel::sql_types::Nullable<diesel::sql_types::Text>, _>(
                    None::<String>,
                );
            }
            for row in plan.load::<Plan>(conn).unwrap() {
                eprintln!("{}", row.detail);
            }
        }
    }
}

fn seed_excluded_receipts(
    conn: &mut SqliteConnection,
    (owner, other): (&Account, &Account),
    [carried, recent, deleted, foreign]: [Uuid; 4],
    before: NaiveDateTime,
    cutoff: NaiveDateTime,
) {
    use TransactionCategory::*;
    // A current-month round trip does not change the reference. Neither does a late fee.
    for (category, amount, when) in [
        (Deposit, dec!(10000), before),
        (FundTrade(recent), dec!(1000), cutoff),
        (PaymentFromTrade(recent), dec!(900), cutoff),
        (CloseSafetyStop(recent), dec!(900), cutoff),
        (CloseTarget(carried), dec!(800), before),
        (FeeClose(carried), dec!(1.25), cutoff),
    ] {
        record(conn, owner, Currency::USD, category, amount, when);
    }
    // A reservation in another currency/account must not make a USD receipt carried.
    for (account, currency, category, amount, when) in [
        (owner, Currency::EUR, FundTrade(recent), dec!(1000), before),
        (other, Currency::USD, FundTrade(foreign), dec!(1000), before),
        (other, Currency::USD, Deposit, dec!(10000), cutoff),
        (
            owner,
            Currency::USD,
            CloseTarget(foreign),
            dec!(1100),
            cutoff,
        ),
    ] {
        record(conn, account, currency, category, amount, when);
    }
    let removed = record(
        conn,
        owner,
        Currency::USD,
        FundTrade(deleted),
        dec!(1000),
        before,
    );
    diesel::update(transactions::table.filter(transactions::id.eq(removed.id.to_string())))
        .set(transactions::deleted_at.eq(Some(cutoff)))
        .execute(conn)
        .unwrap();
    record(
        conn,
        owner,
        Currency::USD,
        CloseTarget(deleted),
        dec!(1100),
        cutoff,
    );
}
