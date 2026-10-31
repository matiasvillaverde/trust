//! Snapshot-consistent, bounded database history readers.
use crate::{
    error::IntoDomainModel,
    schema::{distribution_history, trades, trading_vehicles, transactions},
    workers::{
        worker_distribution::DistributionHistorySQLite,
        worker_transaction::{TransactionSQLite, CASH_FLOW_CATEGORIES},
        WorkerTrade,
    },
};
use diesel::{
    prelude::*,
    sql_types::{Bool, Text},
};
use model::{
    history::{HistoryPage, PageRequest, TradeHistoryFilter, TransactionHistoryFilter},
    DistributionHistory, Trade, Transaction,
};
use std::error::Error;
use uuid::Uuid;

pub(crate) fn trades(
    connection: &mut SqliteConnection,
    account: Uuid,
    filter: &TradeHistoryFilter,
    page: PageRequest,
) -> Result<HistoryPage<Trade>, Box<dyn Error>> {
    let query = || {
        let mut query = trades::table
            .inner_join(trading_vehicles::table)
            .filter(trades::account_id.eq(account.to_string()))
            .filter(trades::deleted_at.is_null())
            .into_boxed();
        if let Some(status) = filter.status {
            query = query.filter(trades::status.eq(status.to_string()));
        }
        if let Some(symbol) = &filter.symbol {
            query = query.filter(
                diesel::dsl::sql::<Bool>("trading_vehicles.symbol COLLATE NOCASE = ")
                    .bind::<Text, _>(symbol),
            );
        }
        if filter.active_only {
            query =
                query.filter(trades::status.eq_any(["submitted", "partially_filled", "filled"]));
        }
        query
    };
    connection.transaction(|connection| {
        let total = query().count().get_result::<i64>(connection)?;
        let ids = query()
            .order((trades::created_at.asc(), trades::id.asc()))
            .select(trades::id)
            .limit(page.limit())
            .offset(page.offset())
            .load::<String>(connection)?;
        let items = load_trades(connection, ids)?;
        page.finish(items, total).map_err(Into::into)
    })
}

pub(crate) fn active_batch(
    connection: &mut SqliteConnection,
    account: Uuid,
    after: Option<Uuid>,
    limit: u16,
) -> Result<Vec<Trade>, Box<dyn Error>> {
    if !(1..=501).contains(&limit) {
        return Err("Reconciliation batch must contain 1..=501 rows".into());
    }
    connection.transaction(|connection| {
        let mut query = trades::table
            .filter(trades::account_id.eq(account.to_string()))
            .filter(trades::deleted_at.is_null())
            .filter(trades::status.eq_any(["submitted", "partially_filled", "filled"]))
            .into_boxed();
        if let Some(after) = after {
            query = query.filter(trades::id.gt(after.to_string()));
        }
        let ids = query
            .order(trades::id.asc())
            .select(trades::id)
            .limit(i64::from(limit))
            .load::<String>(connection)?;
        load_trades(connection, ids)
    })
}

fn load_trades(
    connection: &mut SqliteConnection,
    ids: Vec<String>,
) -> Result<Vec<Trade>, Box<dyn Error>> {
    ids.into_iter()
        .map(|id| WorkerTrade::read_trade(connection, Uuid::parse_str(&id)?))
        .collect()
}

pub(crate) fn transactions(
    connection: &mut SqliteConnection,
    account: Uuid,
    filter: &TransactionHistoryFilter,
    page: PageRequest,
) -> Result<HistoryPage<Transaction>, Box<dyn Error>> {
    if filter.from > filter.to {
        return Err("History start must not exceed end".into());
    }
    let query = || {
        transactions::table
            .filter(transactions::account_id.eq(account.to_string()))
            .filter(transactions::deleted_at.is_null())
            .filter(transactions::category.eq_any(CASH_FLOW_CATEGORIES))
            .filter(transactions::created_at.ge(filter.from))
            .filter(transactions::created_at.le(filter.to))
    };
    connection.transaction(|connection| {
        let total = query().count().get_result::<i64>(connection)?;
        let items = query()
            .order((transactions::created_at.asc(), transactions::id.asc()))
            .limit(page.limit())
            .offset(page.offset())
            .load::<TransactionSQLite>(connection)?
            .into_iter()
            .map(IntoDomainModel::into_domain_model)
            .collect::<Result<_, _>>()?;
        page.finish(items, total).map_err(Into::into)
    })
}

pub(crate) fn distributions(
    connection: &mut SqliteConnection,
    account: Uuid,
    page: PageRequest,
) -> Result<HistoryPage<DistributionHistory>, Box<dyn Error>> {
    let query = || {
        distribution_history::table
            .filter(distribution_history::source_account_id.eq(account.to_string()))
    };
    connection.transaction(|connection| {
        let total = query().count().get_result::<i64>(connection)?;
        let items = query()
            .order((
                distribution_history::distribution_date.desc(),
                distribution_history::id.desc(),
            ))
            .limit(page.limit())
            .offset(page.offset())
            .load::<DistributionHistorySQLite>(connection)?
            .into_iter()
            .map(IntoDomainModel::into_domain_model)
            .collect::<Result<_, _>>()?;
        page.finish(items, total).map_err(Into::into)
    })
}
