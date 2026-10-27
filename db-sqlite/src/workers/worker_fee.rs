use crate::schema::fee_activities;
use diesel::prelude::*;
use model::FeeActivity;
use std::error::Error;
use uuid::Uuid;

pub struct WorkerFee;

#[derive(Queryable, Insertable)]
#[diesel(table_name = fee_activities)]
pub(crate) struct FeeRow {
    pub id: String,
    pub broker: String,
    pub account_id: String,
    pub broker_activity_id: String,
    pub broker_order_id: Option<String>,
    pub amount: String,
    pub occurred_at: chrono::NaiveDateTime,
    pub trade_id: Option<String>,
}

impl WorkerFee {
    pub fn record(
        conn: &mut SqliteConnection,
        fee: &FeeActivity,
        trade_id: Option<Uuid>,
    ) -> Result<bool, Box<dyn Error>> {
        if fee.broker_activity_id.is_empty() || fee.amount <= rust_decimal::Decimal::ZERO {
            return Err("Fee identity and positive amount are required".into());
        }
        crate::atomic::savepoint(conn, |conn| {
            let row = FeeRow {
                id: Uuid::new_v4().to_string(),
                broker: fee.broker.clone(),
                account_id: fee.account_id.to_string(),
                broker_activity_id: fee.broker_activity_id.clone(),
                broker_order_id: fee.broker_order_id.clone(),
                amount: fee.amount.normalize().to_string(),
                occurred_at: fee.occurred_at,
                trade_id: trade_id.map(|id| id.to_string()),
            };
            let inserted = diesel::insert_into(fee_activities::table)
                .values(&row)
                .on_conflict((
                    fee_activities::broker,
                    fee_activities::account_id,
                    fee_activities::broker_activity_id,
                ))
                .do_nothing()
                .execute(conn)?;
            if inserted != 0 {
                return Ok(trade_id.is_some());
            }
            let existing: FeeRow = fee_activities::table
                .filter(fee_activities::broker.eq(&row.broker))
                .filter(fee_activities::account_id.eq(&row.account_id))
                .filter(fee_activities::broker_activity_id.eq(&row.broker_activity_id))
                .first(conn)?;
            if existing.amount != row.amount
                || existing.broker_order_id != row.broker_order_id
                || existing.occurred_at != row.occurred_at
            {
                return Err("Conflicting broker fee activity identity; reconcile manually".into());
            }
            match (existing.trade_id.as_ref(), row.trade_id.as_ref()) {
                (Some(previous), Some(next)) if previous != next => {
                    Err("Fee activity already belongs to another trade".into())
                }
                (None, Some(next)) => {
                    diesel::update(
                        fee_activities::table.filter(fee_activities::id.eq(existing.id)),
                    )
                    .set(fee_activities::trade_id.eq(next))
                    .execute(conn)?;
                    Ok(true)
                }
                _ => Ok(false),
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use model::{DatabaseFactory, Environment};

    #[test]
    fn fee_identity_is_idempotent_and_conflicting_replay_is_rejected() {
        let database = crate::SqliteDatabase::new_in_memory();
        let account = database
            .account_write()
            .create(
                "fees",
                "test",
                Environment::Paper,
                rust_decimal::Decimal::ZERO,
                rust_decimal::Decimal::ZERO,
            )
            .unwrap();
        let mut fee = FeeActivity {
            broker: "alpaca".into(),
            account_id: account.id,
            broker_activity_id: "activity-1".into(),
            broker_order_id: None,
            amount: rust_decimal::Decimal::ONE,
            occurred_at: chrono::Utc::now().naive_utc(),
            symbol: None,
            activity_type: "FEE".into(),
            raw_json: None,
        };
        assert!(!database
            .execution_write()
            .record_fee_activity(&fee, None)
            .unwrap());
        assert!(!database
            .execution_write()
            .record_fee_activity(&fee, None)
            .unwrap());
        fee.amount = rust_decimal::Decimal::TWO;
        assert!(database
            .execution_write()
            .record_fee_activity(&fee, None)
            .is_err());
    }
}
