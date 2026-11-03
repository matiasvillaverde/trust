//! Fixed query shapes reuse prepared statements. Current closes are only needed
//! when earlier reservation movements exist, and every branch shares a snapshot.
use super::*;

pub(super) fn read(
    connection: &mut SqliteConnection,
    account_id: Uuid,
    currency: &Currency,
    cutoff: NaiveDateTime,
) -> Result<Vec<TransactionSQLite>, Box<dyn Error>> {
    crate::atomic::savepoint(connection, |connection| {
        let scoped = transactions::table
            .filter(transactions::deleted_at.is_null())
            .filter(transactions::account_id.eq(account_id.to_string()))
            .filter(transactions::currency.eq(currency.to_string()));
        let prior = scoped
            .clone()
            .filter(transactions::created_at.lt(cutoff))
            .filter(
                transactions::category
                    .eq("fund_trade")
                    .or(transactions::category.eq("payment_from_trade")),
            );
        let mut rows = prior.load::<TransactionSQLite>(connection)?;
        let has_prior_movements = !rows.is_empty();
        rows.extend(
            scoped
                .clone()
                .filter(transactions::created_at.ge(cutoff))
                .filter(
                    transactions::category
                        .eq("deposit")
                        .or(transactions::category.eq("withdrawal"))
                        .or(transactions::category.eq("withdrawal_earnings"))
                        .or(transactions::category.eq("payment_earnings")),
                )
                .load::<TransactionSQLite>(connection)?,
        );
        if has_prior_movements {
            rows.extend(closing_receipts(connection, account_id, currency, cutoff)?);
        }
        Ok(rows)
    })
}

fn closing_receipts(
    connection: &mut SqliteConnection,
    account_id: Uuid,
    currency: &Currency,
    cutoff: NaiveDateTime,
) -> Result<Vec<TransactionSQLite>, Box<dyn Error>> {
    let reservations = diesel::alias!(transactions as reservations);
    let prior_ids = reservations
        .filter(reservations.field(transactions::deleted_at).is_null())
        .filter(
            reservations
                .field(transactions::account_id)
                .eq(account_id.to_string()),
        )
        .filter(
            reservations
                .field(transactions::currency)
                .eq(currency.to_string()),
        )
        .filter(reservations.field(transactions::created_at).lt(cutoff))
        .filter(
            reservations
                .field(transactions::category)
                .eq("fund_trade")
                .or(reservations
                    .field(transactions::category)
                    .eq("payment_from_trade")),
        )
        .select(reservations.field(transactions::trade_id));
    Ok(transactions::table
        .filter(transactions::deleted_at.is_null())
        .filter(transactions::account_id.eq(account_id.to_string()))
        .filter(transactions::currency.eq(currency.to_string()))
        .filter(transactions::created_at.ge(cutoff))
        .filter(
            transactions::category
                .eq("close_target")
                .or(transactions::category.eq("close_safety_stop"))
                .or(transactions::category.eq("close_safety_stop_slippage")),
        )
        .filter(transactions::trade_id.eq_any(prior_ids))
        .load(connection)?)
}
