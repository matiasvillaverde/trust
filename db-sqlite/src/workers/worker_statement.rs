//! Bounded evidence from one financial snapshot; no balance projections or writes.
use chrono::NaiveDateTime;
use diesel::{
    prelude::*,
    sql_types::{Nullable, Text, Timestamp},
};
use model::statement::*;
use rust_decimal::Decimal;
use std::{collections::BTreeSet, error::Error};
use uuid::Uuid;

#[derive(QueryableByName)]
struct ExecutionRow {
    #[diesel(sql_type = Text)]
    account_id: String,
    #[diesel(sql_type = Nullable<Text>)]
    trade_id: Option<String>,
    #[diesel(sql_type = Text)]
    broker: String,
    #[diesel(sql_type = Text)]
    source: String,
    #[diesel(sql_type = Text)]
    broker_execution_id: String,
    #[diesel(sql_type = Text)]
    symbol: String,
    #[diesel(sql_type = Text)]
    side: String,
    #[diesel(sql_type = Text)]
    qty: String,
    #[diesel(sql_type = Text)]
    price: String,
    #[diesel(sql_type = Timestamp)]
    executed_at: NaiveDateTime,
    #[diesel(sql_type = Nullable<Text>)]
    currency: Option<String>,
    #[diesel(sql_type = Nullable<Text>)]
    instrument_id: Option<String>,
    #[diesel(sql_type = Nullable<Text>)]
    asset_class: Option<String>,
}

impl TryFrom<ExecutionRow> for RecordedExecution {
    type Error = Box<dyn Error>;
    fn try_from(row: ExecutionRow) -> Result<Self, Self::Error> {
        if row.broker != "ibkr"
            || row.source != "account_activities"
            || !matches!(row.asset_class.as_deref(), Some("stock" | "etf"))
        {
            return Err("Unsupported or unlinked statement execution".into());
        }
        let buy = match row.side.as_str() {
            "buy" => true,
            "sell" | "sell_short" => false,
            _ => return Err("Invalid statement execution side".into()),
        };
        Ok(Self {
            account_id: Uuid::parse_str(&row.account_id)?,
            trade_id: Uuid::parse_str(row.trade_id.as_deref().ok_or("Unlinked execution")?)?,
            execution: StatementExecution {
                execution_id: row.broker_execution_id,
                instrument_id: row
                    .instrument_id
                    .filter(|id| !id.is_empty())
                    .ok_or("Missing contract identity")?,
                symbol: row.symbol,
                currency: row
                    .currency
                    .ok_or("Missing execution currency")?
                    .to_ascii_uppercase(),
                buy,
                quantity: Decimal::from_str_exact(&row.qty)?,
                price: Decimal::from_str_exact(&row.price)?,
                occurred_at: row.executed_at.and_utc(),
                commission: Decimal::ZERO,
            },
        })
    }
}

#[derive(QueryableByName)]
struct FeeRow {
    #[diesel(sql_type = Text)]
    account_id: String,
    #[diesel(sql_type = Nullable<Text>)]
    trade_id: Option<String>,
    #[diesel(sql_type = Text)]
    broker: String,
    #[diesel(sql_type = Text)]
    broker_activity_id: String,
    #[diesel(sql_type = Text)]
    amount: String,
    #[diesel(sql_type = Timestamp)]
    occurred_at: NaiveDateTime,
}

#[derive(QueryableByName)]
struct MovementRow {
    #[diesel(sql_type = Text)]
    account_id: String,
    #[diesel(sql_type = Nullable<Text>)]
    trade_id: Option<String>,
    #[diesel(sql_type = Text)]
    category: String,
    #[diesel(sql_type = Text)]
    currency: String,
    #[diesel(sql_type = Text)]
    amount: String,
    #[diesel(sql_type = Timestamp)]
    created_at: NaiveDateTime,
}

fn bounded<T>(rows: Vec<T>) -> Result<Vec<T>, Box<dyn Error>> {
    if rows.len() > MAX_LEDGER_ROWS {
        return Err("Statement evidence exceeds the bounded ledger limit".into());
    }
    Ok(rows)
}

#[derive(QueryableByName)]
struct TradeRow {
    #[diesel(sql_type = Text)]
    account_id: String,
    #[diesel(sql_type = Text)]
    trade_id: String,
    #[diesel(sql_type = Text)]
    category: String,
    #[diesel(sql_type = Text)]
    entry_filled: String,
    #[diesel(sql_type = Text)]
    stop_filled: String,
    #[diesel(sql_type = Text)]
    target_filled: String,
}

fn trades(
    connection: &mut SqliteConnection,
    ids: &str,
) -> Result<Vec<RecordedTrade>, Box<dyn Error>> {
    bounded(
        diesel::sql_query(
            "SELECT t.account_id,t.id AS trade_id,t.category,
        CASE WHEN e.id IS NOT NULL THEN coalesce(e.filled_quantity,'0') END AS entry_filled,
        CASE WHEN s.id IS NOT NULL THEN coalesce(s.filled_quantity,'0') END AS stop_filled,
        CASE WHEN x.id IS NOT NULL THEN coalesce(x.filled_quantity,'0') END AS target_filled
        FROM trades t LEFT JOIN orders e ON e.id=t.entry_id AND e.deleted_at IS NULL
        LEFT JOIN orders s ON s.id=t.safety_stop_id AND s.deleted_at IS NULL
        LEFT JOIN orders x ON x.id=t.target_id AND x.deleted_at IS NULL
        WHERE t.deleted_at IS NULL AND t.account_id IN (SELECT value FROM json_each(?1))
        ORDER BY t.id LIMIT 20001",
        )
        .bind::<Text, _>(ids)
        .load::<TradeRow>(connection)?,
    )?
    .into_iter()
    .map(|row| {
        let entry_buy = match row.category.as_str() {
            "long" => true,
            "short" => false,
            _ => return Err("Invalid statement trade direction".into()),
        };
        let stop = Decimal::from_str_exact(&row.stop_filled)?;
        let target = Decimal::from_str_exact(&row.target_filled)?;
        if stop < Decimal::ZERO || target < Decimal::ZERO {
            return Err("Invalid trade fill quantity".into());
        }
        Ok(RecordedTrade {
            account_id: row.account_id.parse()?,
            trade_id: row.trade_id.parse()?,
            entry_buy,
            entry_filled: Decimal::from_str_exact(&row.entry_filled)?,
            exit_filled: stop.checked_add(target).ok_or("Trade fill overflow")?,
        })
    })
    .collect()
}

pub(crate) fn read_statement_ledger(
    connection: &mut SqliteConnection,
    accounts: &BTreeSet<Uuid>,
) -> Result<StatementLedger, Box<dyn Error>> {
    if accounts.is_empty() || accounts.len() > 64 {
        return Err("Statement account scope must contain 1..64 profiles".into());
    }
    let ids = serde_json::to_string(accounts)?;
    crate::atomic::savepoint(connection, |conn| {
        // IBKR trade_updates are derived order-snapshot deltas. Native activities
        // cover the same fills; counting both would double cash and quantities.
        // Core separately requires native quantities to cover every filled trade.
        let executions = bounded(diesel::sql_query(
                "SELECT e.account_id,t.id AS trade_id,e.broker,e.source,e.broker_execution_id,e.symbol,
                 e.side,e.qty,e.price,e.executed_at,t.currency,v.broker_asset_id AS instrument_id,
                 v.category AS asset_class FROM executions e
                 LEFT JOIN trades t ON t.id=e.trade_id AND t.account_id=e.account_id AND t.deleted_at IS NULL
                 LEFT JOIN trading_vehicles v ON v.id=t.trading_vehicle_id AND v.deleted_at IS NULL
                 WHERE e.deleted_at IS NULL AND e.source <> 'trade_updates'
                 AND e.account_id IN (SELECT value FROM json_each(?1))
                 ORDER BY e.executed_at,e.id LIMIT 20001"
            ).bind::<Text,_>(&ids).load::<ExecutionRow>(conn)?)?
                .into_iter().map(RecordedExecution::try_from).collect::<Result<Vec<_>,_>>()?;
        let fees = bounded(diesel::sql_query(
                "SELECT account_id,trade_id,broker,broker_activity_id,amount,occurred_at FROM fee_activities
                 WHERE account_id IN (SELECT value FROM json_each(?1)) ORDER BY occurred_at,id LIMIT 20001"
            ).bind::<Text,_>(&ids).load::<FeeRow>(conn)?)?
                .into_iter().map(|row| -> Result<RecordedFee,Box<dyn Error>> {
                    if row.broker != "ibkr" { return Err("Unsupported statement fee broker".into()); }
                    Ok(RecordedFee {
                        account_id: Uuid::parse_str(&row.account_id)?,
                        trade_id: row.trade_id.as_deref().map(Uuid::parse_str).transpose()?,
                        activity_id: row.broker_activity_id,
                        amount: Decimal::from_str_exact(&row.amount)?,
                        occurred_at: row.occurred_at.and_utc(),
                    })
                }).collect::<Result<Vec<_>,_>>()?;
        let movements = bounded(diesel::sql_query(
                "SELECT account_id,trade_id,category,currency,amount,created_at FROM transactions
                 WHERE deleted_at IS NULL AND account_id IN (SELECT value FROM json_each(?1))
                 AND category IN ('deposit','withdrawal','withdrawal_earnings','withdrawal_tax','fee_open','fee_close')
                 ORDER BY created_at,id LIMIT 20001"
            ).bind::<Text,_>(&ids).load::<MovementRow>(conn)?)?
                .into_iter().map(|row| -> Result<RecordedMovement,Box<dyn Error>> {
                    Ok(RecordedMovement {
                        account_id: Uuid::parse_str(&row.account_id)?,
                        trade_id: row.trade_id.as_deref().map(Uuid::parse_str).transpose()?,
                        category: row.category,
                        currency: row.currency.to_ascii_uppercase(),
                        amount: Decimal::from_str_exact(&row.amount)?,
                        occurred_at: row.created_at.and_utc(),
                    })
                }).collect::<Result<Vec<_>,_>>()?;
        Ok(StatementLedger {
            trades: trades(conn, &ids)?,
            executions,
            fees,
            movements,
        })
    })
}
