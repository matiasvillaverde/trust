//! Fold an appended receipt inside SQLite's own statement/rollback boundary.
//! An invalid receipt poisons the derived total; only a complete replay can
//! establish it again. No policy decision or floating-point SQL sum lives here.
use diesel::{
    prelude::*,
    sql_types::{Binary, Integer, Nullable, Text},
};
use model::TransactionCategory;
use rust_decimal::Decimal;
use uuid::Uuid;

type Arguments = (
    Option<String>,
    String,
    String,
    Option<String>,
    Vec<u8>,
    Vec<u8>,
    i32,
);

pub(super) fn register(conn: &mut SqliteConnection) -> QueryResult<()> {
    conn.register_sql_function::<(
        Nullable<Text>,
        Text,
        Text,
        Nullable<Text>,
        Binary,
        Binary,
        Integer,
    ), Nullable<Text>, Arguments, Option<String>, _>("trust_cash_append", true, fold)
}

#[tracing::instrument(
    level = "trace",
    target = "trust::perf",
    name = "db.cash_projection.append",
    skip_all
)]
fn fold((total, category, amount, trade, date, id, text_keys): Arguments) -> Option<String> {
    if text_keys != 1 || std::str::from_utf8(&date).is_err() || std::str::from_utf8(&id).is_err() {
        return None;
    }
    let total = Decimal::from_str_exact(total.as_deref()?).ok()?;
    let amount = Decimal::from_str_exact(&amount).ok()?;
    let trade = trade.as_deref().map(Uuid::parse_str).transpose().ok()?;
    let category = TransactionCategory::parse(&category, trade).ok()?;
    category
        .apply_to_available_cash(total, amount)
        .ok()
        .map(|value| value.to_string())
}
