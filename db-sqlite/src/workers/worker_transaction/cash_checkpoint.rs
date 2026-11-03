//! Incremental replay of an unchanged cash-ledger prefix. No durable balance
//! projection is trusted: a new connection always starts with a full replay.
use super::WorkerTransaction;
use diesel::{
    prelude::*,
    sql_types::{BigInt, Nullable, Text},
};
use model::{Currency, TransactionCategory};
use rust_decimal::Decimal;
use std::error::Error;
use uuid::Uuid;

mod append;
mod cursor_text;
mod lookup;
mod schema;
use cursor_text::CursorText;

#[derive(QueryableByName)]
struct DataVersion {
    #[diesel(sql_type = BigInt)]
    data_version: i64,
}

#[derive(QueryableByName)]
struct Checkpoint {
    #[diesel(sql_type = Text)]
    total: String,
    #[diesel(sql_type = Nullable<Text>)]
    last_created_at: Option<String>,
    #[diesel(sql_type = Nullable<Text>)]
    last_id: Option<String>,
}

#[derive(QueryableByName)]
struct CashRow {
    #[diesel(sql_type = Text)]
    category: String,
    #[diesel(sql_type = Text)]
    amount: String,
    #[diesel(sql_type = Nullable<Text>)]
    trade_id: Option<String>,
    #[diesel(sql_type = Text)]
    #[diesel(deserialize_as = CursorText)]
    created_at: String,
    #[diesel(sql_type = Text)]
    #[diesel(deserialize_as = CursorText)]
    id: String,
}

const CASH_SELECT: &str = "SELECT category, amount, trade_id, created_at, id FROM main.transactions
WHERE deleted_at IS NULL AND account_id = ?1 AND currency = ?2
AND category IN ('deposit', 'withdrawal', 'withdrawal_earnings', 'fee_open', 'fee_close',
                'fund_trade', 'payment_from_trade', 'payment_earnings')";

#[tracing::instrument(
    level = "trace",
    target = "trust::perf",
    name = "db.cash_movements",
    skip_all,
    fields(rows)
)]
pub(crate) fn calculate(
    conn: &mut SqliteConnection,
    account_id: Uuid,
    currency: &Currency,
) -> Result<Decimal, Box<dyn Error>> {
    if let Some(total) = lookup::read(conn, &account_id.to_string(), &currency.to_string())? {
        tracing::Span::current().record("rows", 0_u64);
        return Ok(total);
    }
    if !schema::supported(conn)? {
        return WorkerTransaction::read_cash_movements(conn, account_id, currency)?
            .into_iter()
            .try_fold(Decimal::ZERO, |total, (category, amount)| {
                category
                    .apply_to_available_cash(total, amount)
                    .map_err(Into::into)
            });
    }
    crate::atomic::savepoint(conn, |conn| {
        // Establish a main-database read snapshot before validating data_version.
        // TEMP reads alone do not establish that authoritative snapshot.
        crate::schema::transactions::table
            .select(diesel::dsl::sql::<BigInt>("rowid"))
            .limit(1)
            .load::<i64>(conn)?;
        let current = version(conn)?;
        schema::ensure(conn)?;
        replay(conn, account_id, currency, current)
    })
}

fn version(conn: &mut SqliteConnection) -> Result<i64, Box<dyn Error>> {
    Ok(diesel::sql_query("PRAGMA main.data_version")
        .get_result::<DataVersion>(conn)?
        .data_version)
}

fn replay(
    conn: &mut SqliteConnection,
    account_id: Uuid,
    currency: &Currency,
    version: i64,
) -> Result<Decimal, Box<dyn Error>> {
    let account = account_id.to_string();
    let currency = currency.to_string();
    let schema_version = schema::version(conn)?;
    let checkpoint = diesel::sql_query(
        "SELECT total, last_created_at, last_id FROM temp.trust_verified_cash
      WHERE account_id = ?1 AND currency = ?2 AND data_version = ?3 AND total IS NOT NULL
        AND schema_version = ?4",
    )
    .bind::<Text, _>(&account)
    .bind::<Text, _>(&currency)
    .bind::<BigInt, _>(version)
    .bind::<BigInt, _>(schema_version)
    .get_result::<Checkpoint>(conn)
    .optional()?;
    let mut total = checkpoint
        .as_ref()
        .map(|value| Decimal::from_str_exact(&value.total))
        .transpose()?
        .unwrap_or(Decimal::ZERO);
    let boundary = checkpoint
        .as_ref()
        .and_then(|value| value.last_created_at.as_ref().zip(value.last_id.as_ref()));
    let rows = tracing::trace_span!(target: "trust::perf", "db.ledger.load")
        .in_scope(|| read_rows(conn, &account, &currency, boundary))?;
    tracing::Span::current().record("rows", rows.len());
    {
        let _decode =
            tracing::trace_span!(target: "trust::perf", "db.ledger.decode_cash").entered();
        for row in &rows {
            let trade_id = row.trade_id.as_deref().map(Uuid::parse_str).transpose()?;
            let category = TransactionCategory::parse(&row.category, trade_id)
                .map_err(|_| "Invalid cash movement category or trade identity")?;
            total =
                category.apply_to_available_cash(total, Decimal::from_str_exact(&row.amount)?)?;
        }
    }
    let boundary = rows
        .last()
        .map(|row| (&row.created_at, &row.id))
        .or(boundary);
    diesel::sql_query(
        "INSERT INTO temp.trust_verified_cash
          (account_id, currency, data_version, total, last_created_at, last_id, schema_version)
          VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
          ON CONFLICT(account_id, currency) DO UPDATE SET data_version=excluded.data_version,
          total=excluded.total, last_created_at=excluded.last_created_at, last_id=excluded.last_id,
          schema_version=excluded.schema_version, pending_id=NULL",
    )
    .bind::<Text, _>(&account)
    .bind::<Text, _>(&currency)
    .bind::<BigInt, _>(version)
    .bind::<Text, _>(total.to_string())
    .bind::<Nullable<Text>, _>(boundary.map(|(date, _)| date))
    .bind::<Nullable<Text>, _>(boundary.map(|(_, id)| id))
    .bind::<BigInt, _>(schema_version)
    .execute(conn)?;
    Ok(total)
}

fn read_rows(
    conn: &mut SqliteConnection,
    account: &str,
    currency: &str,
    boundary: Option<(&String, &String)>,
) -> Result<Vec<CashRow>, Box<dyn Error>> {
    // Row-value comparison lets the chronological index seek directly to the
    // prefix boundary instead of filtering a full account scan in Rust.
    match boundary {
        Some((date, id)) => Ok(diesel::sql_query(format!(
            "{CASH_SELECT} AND (created_at, id) > (?3, ?4) ORDER BY created_at, id"
        ))
        .bind::<Text, _>(account)
        .bind::<Text, _>(currency)
        .bind::<Text, _>(date)
        .bind::<Text, _>(id)
        .load(conn)?),
        None => Ok(
            diesel::sql_query(format!("{CASH_SELECT} ORDER BY created_at, id"))
                .bind::<Text, _>(account)
                .bind::<Text, _>(currency)
                .load(conn)?,
        ),
    }
}
