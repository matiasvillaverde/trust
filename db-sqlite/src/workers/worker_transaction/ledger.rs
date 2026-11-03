//! Ordered, exact-decimal ledger reads used by admission and reporting.
use super::{TransactionSQLite, WorkerTransaction};
use crate::{error::IntoDomainModels, schema::transactions};
use chrono::{Datelike, NaiveDate, NaiveDateTime, NaiveTime, Utc};
use diesel::prelude::*;
use model::{Currency, Transaction, TransactionCategory};
use rust_decimal::Decimal;
use std::error::Error;
use tracing::error;
use uuid::Uuid;

pub(crate) const CASH_FLOW_CATEGORIES: [&str; 8] = [
    "deposit",
    "withdrawal",
    "withdrawal_earnings",
    "fee_open",
    "fee_close",
    "fund_trade",
    "payment_from_trade",
    "payment_earnings",
];

mod monthly;

impl WorkerTransaction {
    #[tracing::instrument(
        level = "trace",
        target = "trust::perf",
        name = "db.monthly_reference_inputs",
        skip_all,
        fields(rows)
    )]
    pub fn read_monthly_reference_inputs(
        connection: &mut SqliteConnection,
        account_id: Uuid,
        currency: &Currency,
        cutoff: NaiveDateTime,
    ) -> Result<Vec<Transaction>, Box<dyn Error>> {
        let mut rows = tracing::trace_span!(target: "trust::perf", "db.ledger.load")
            .in_scope(|| monthly::read(connection, account_id, currency, cutoff))?;
        // Sort the selected receipts after indexed filtering, preserving the
        // ordered accounting contract across the disjoint query branches.
        rows.sort_by(|left, right| {
            left.created_at
                .cmp(&right.created_at)
                .then_with(|| left.id.cmp(&right.id))
        });
        tracing::Span::current().record("rows", rows.len());
        decode_transactions(rows)
    }

    /// Preserve the ordered ledger calculation without decoding report metadata on
    /// every sizing request. No SQL floating-point sum or cached balance is used.
    #[tracing::instrument(
        level = "trace",
        target = "trust::perf",
        name = "db.cash_movements",
        skip_all,
        fields(rows)
    )]
    pub fn read_cash_movements(
        connection: &mut SqliteConnection,
        account_id: Uuid,
        currency: &Currency,
    ) -> Result<Vec<(TransactionCategory, Decimal)>, Box<dyn Error>> {
        let rows =
            tracing::trace_span!(target: "trust::perf", "db.ledger.load").in_scope(|| {
                transactions::table
                    .filter(transactions::deleted_at.is_null())
                    .filter(transactions::account_id.eq(account_id.to_string()))
                    .filter(transactions::currency.eq(currency.to_string()))
                    .filter(transactions::category.eq_any(CASH_FLOW_CATEGORIES))
                    .order((transactions::created_at.asc(), transactions::id.asc()))
                    .select((
                        transactions::category,
                        transactions::amount,
                        transactions::trade_id,
                    ))
                    .load::<(String, String, Option<String>)>(connection)
            })?;
        tracing::Span::current().record("rows", rows.len());
        decode_cash(rows)
    }

    #[tracing::instrument(
        level = "trace",
        target = "trust::perf",
        name = "db.all_transactions",
        skip_all,
        fields(rows)
    )]
    pub fn read_all_transactions(
        connection: &mut SqliteConnection,
        account_id: Uuid,
        currency: &Currency,
    ) -> Result<Vec<Transaction>, Box<dyn Error>> {
        let rows =
            tracing::trace_span!(target: "trust::perf", "db.ledger.load").in_scope(|| {
                transactions::table
                    .filter(transactions::deleted_at.is_null())
                    .filter(transactions::account_id.eq(account_id.to_string()))
                    .filter(transactions::currency.eq(currency.to_string()))
                    .load::<TransactionSQLite>(connection)
                    .map_err(|error| {
                        error!("Error reading all transactions: {:?}", error);
                        error
                    })
            })?;
        tracing::Span::current().record("rows", rows.len());
        decode_transactions(rows)
    }

    #[tracing::instrument(
        level = "trace",
        target = "trust::perf",
        name = "db.account_transactions",
        skip_all,
        fields(rows)
    )]
    pub fn read_all_trade_transactions_excluding_taxes(
        connection: &mut SqliteConnection,
        account_id: Uuid,
        currency: &Currency,
    ) -> Result<Vec<Transaction>, Box<dyn Error>> {
        // Hot path: avoid N queries per category; fetch via a single `IN (...)` query.
        // This intentionally excludes tax reserve flows; use `read_all_account_transactions_taxes`
        // for tax-specific reporting.
        // Use string literals to avoid borrowing from temporary enum values.

        let rows =
            tracing::trace_span!(target: "trust::perf", "db.ledger.load").in_scope(|| {
                transactions::table
                    .filter(transactions::deleted_at.is_null())
                    .filter(transactions::account_id.eq(account_id.to_string()))
                    .filter(transactions::currency.eq(currency.to_string()))
                    .filter(transactions::category.eq_any(CASH_FLOW_CATEGORIES))
                    .order((transactions::created_at.asc(), transactions::id.asc()))
                    .load::<TransactionSQLite>(connection)
                    .map_err(|error| {
                        error!(
                            "Error reading all transactions excluding taxes: {:?}",
                            error
                        );
                        error
                    })
            })?;

        tracing::Span::current().record("rows", rows.len());
        decode_transactions(rows)
    }

    #[tracing::instrument(
        level = "trace",
        target = "trust::perf",
        name = "db.opening_transactions",
        skip_all,
        fields(rows)
    )]
    pub fn read_all_transaction_excluding_current_month_and_taxes(
        connection: &mut SqliteConnection,
        account_id: Uuid,
        currency: &Currency,
    ) -> Result<Vec<Transaction>, Box<dyn Error>> {
        let now = Utc::now().naive_utc();
        let first_day_of_month =
            NaiveDate::from_ymd_opt(now.year(), now.month(), 1).ok_or("Failed to create date")?;
        let first_day_of_month = NaiveDateTime::new(
            first_day_of_month,
            NaiveTime::from_hms_opt(0, 0, 0).ok_or("Failed to create time")?,
        );

        // Keep this aligned with the calculator(s) that consume it.
        let included_categories: [&str; 8] = [
            "deposit",
            "withdrawal",
            "withdrawal_earnings",
            "fee_open",
            "fee_close",
            "fund_trade",
            "payment_from_trade",
            "payment_earnings",
        ];

        let rows =
            tracing::trace_span!(target: "trust::perf", "db.ledger.load").in_scope(|| {
                transactions::table
                    .filter(transactions::deleted_at.is_null())
                    .filter(transactions::account_id.eq(account_id.to_string()))
                    .filter(transactions::created_at.lt(first_day_of_month))
                    .filter(transactions::currency.eq(currency.to_string()))
                    .filter(transactions::category.eq_any(included_categories))
                    .order((transactions::created_at.asc(), transactions::id.asc()))
                    .load::<TransactionSQLite>(connection)
                    .map_err(|error| {
                        error!(
                            "Error reading beginning-of-month transactions excluding taxes: {:?}",
                            error
                        );
                        error
                    })
            })?;

        tracing::Span::current().record("rows", rows.len());
        decode_transactions(rows)
    }
}

#[tracing::instrument(
    level = "trace",
    target = "trust::perf",
    name = "db.ledger.decode_transactions",
    skip_all
)]
fn decode_transactions(rows: Vec<TransactionSQLite>) -> Result<Vec<Transaction>, Box<dyn Error>> {
    rows.into_domain_models()
}

fn decode_cash(
    rows: Vec<(String, String, Option<String>)>,
) -> Result<Vec<(TransactionCategory, Decimal)>, Box<dyn Error>> {
    let _decode = tracing::trace_span!(target: "trust::perf", "db.ledger.decode_cash").entered();
    rows.into_iter()
        .map(|(category, amount, trade_id)| {
            let trade_id = trade_id.as_deref().map(Uuid::parse_str).transpose()?;
            let category = TransactionCategory::parse(&category, trade_id)
                .map_err(|_| "Invalid cash movement category or trade identity")?;
            let amount = Decimal::from_str_exact(&amount)?;
            Ok((category, amount))
        })
        .collect()
}
