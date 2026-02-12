use crate::error::{ConversionError, IntoDomainModel, IntoDomainModels};
use crate::schema::{trade_grades, trades};
use chrono::{Duration, NaiveDateTime, Utc};
use diesel::prelude::*;
use model::{Grade, TradeGrade};
use std::error::Error;
use std::str::FromStr;
use tracing::error;
use uuid::Uuid;

/// Worker for handling trade grade database operations
#[derive(Debug)]
pub struct WorkerTradeGrade;

impl WorkerTradeGrade {
    pub fn create(
        connection: &mut SqliteConnection,
        grade: &TradeGrade,
    ) -> Result<TradeGrade, Box<dyn Error>> {
        let now = Utc::now().naive_utc();
        let record = NewTradeGrade {
            id: grade.id.to_string(),
            created_at: now,
            updated_at: now,
            deleted_at: None,
            trade_id: grade.trade_id.to_string(),
            overall_score: i32::from(grade.overall_score),
            overall_grade: grade.overall_grade.to_string(),
            process_score: i32::from(grade.process_score),
            risk_score: i32::from(grade.risk_score),
            execution_score: i32::from(grade.execution_score),
            documentation_score: i32::from(grade.documentation_score),
            recommendations: if grade.recommendations.is_empty() {
                None
            } else {
                Some(serde_json::to_string(&grade.recommendations)?)
            },
            graded_at: grade.graded_at,
            process_weight_permille: i32::from(grade.process_weight_permille),
            risk_weight_permille: i32::from(grade.risk_weight_permille),
            execution_weight_permille: i32::from(grade.execution_weight_permille),
            documentation_weight_permille: i32::from(grade.documentation_weight_permille),
        };

        diesel::insert_into(trade_grades::table)
            .values(&record)
            .get_result::<TradeGradeSQLite>(connection)
            .map_err(|error| {
                error!("Error creating trade grade: {:?}", error);
                error
            })?
            .into_domain_model()
    }

    pub fn read_latest_for_trade(
        connection: &mut SqliteConnection,
        trade_id: Uuid,
    ) -> Result<Option<TradeGrade>, Box<dyn Error>> {
        let row = trade_grades::table
            .filter(trade_grades::deleted_at.is_null())
            .filter(trade_grades::trade_id.eq(trade_id.to_string()))
            .order_by(trade_grades::graded_at.desc())
            .first::<TradeGradeSQLite>(connection)
            .optional()
            .map_err(|error| {
                error!("Error reading latest trade grade: {:?}", error);
                error
            })?;

        row.map(|sqlite| sqlite.into_domain_model()).transpose()
    }

    pub fn read_for_account_days(
        connection: &mut SqliteConnection,
        account_id: Uuid,
        days: u32,
    ) -> Result<Vec<TradeGrade>, Box<dyn Error>> {
        #[allow(clippy::cast_possible_wrap)]
        let start = Utc::now()
            .naive_utc()
            .checked_sub_signed(Duration::days(i64::from(days)))
            .ok_or_else(|| ConversionError::new("days", "Invalid days window"))?;

        trade_grades::table
            .inner_join(trades::table.on(trades::id.eq(trade_grades::trade_id)))
            .select(TradeGradeSQLite::as_select())
            .filter(trade_grades::deleted_at.is_null())
            .filter(trades::deleted_at.is_null())
            .filter(trades::account_id.eq(account_id.to_string()))
            .filter(trade_grades::graded_at.ge(start))
            .order_by(trade_grades::graded_at.asc())
            .load::<TradeGradeSQLite>(connection)
            .map_err(|error| {
                error!("Error reading trade grades for account: {:?}", error);
                error
            })?
            .into_domain_models()
    }
}

#[derive(Debug, Queryable, Identifiable, Selectable)]
#[diesel(table_name = trade_grades)]
struct TradeGradeSQLite {
    id: String,
    created_at: NaiveDateTime,
    updated_at: NaiveDateTime,
    deleted_at: Option<NaiveDateTime>,
    trade_id: String,
    overall_score: i32,
    overall_grade: String,
    process_score: i32,
    risk_score: i32,
    execution_score: i32,
    documentation_score: i32,
    recommendations: Option<String>,
    graded_at: NaiveDateTime,
    process_weight_permille: i32,
    risk_weight_permille: i32,
    execution_weight_permille: i32,
    documentation_weight_permille: i32,
}

impl TryFrom<TradeGradeSQLite> for TradeGrade {
    type Error = ConversionError;

    fn try_from(value: TradeGradeSQLite) -> Result<Self, Self::Error> {
        let recommendations: Vec<String> = match value.recommendations.as_deref() {
            None => Vec::new(),
            Some(text) => serde_json::from_str(text).map_err(|_| {
                ConversionError::new("recommendations", "Failed to parse recommendations JSON")
            })?,
        };

        let grade = Grade::from_str(&value.overall_grade)
            .map_err(|_| ConversionError::new("overall_grade", "Failed to parse grade"))?;

        Ok(TradeGrade {
            id: Uuid::parse_str(&value.id)
                .map_err(|_| ConversionError::new("id", "Failed to parse trade grade ID"))?,
            created_at: value.created_at,
            updated_at: value.updated_at,
            deleted_at: value.deleted_at,
            trade_id: Uuid::parse_str(&value.trade_id)
                .map_err(|_| ConversionError::new("trade_id", "Failed to parse trade ID"))?,
            overall_score: value
                .overall_score
                .clamp(0, 100)
                .try_into()
                .map_err(|_| ConversionError::new("overall_score", "Invalid overall score"))?,
            overall_grade: grade,
            process_score: value
                .process_score
                .clamp(0, 100)
                .try_into()
                .map_err(|_| ConversionError::new("process_score", "Invalid process score"))?,
            risk_score: value
                .risk_score
                .clamp(0, 100)
                .try_into()
                .map_err(|_| ConversionError::new("risk_score", "Invalid risk score"))?,
            execution_score: value
                .execution_score
                .clamp(0, 100)
                .try_into()
                .map_err(|_| ConversionError::new("execution_score", "Invalid execution score"))?,
            documentation_score: value.documentation_score.clamp(0, 100).try_into().map_err(
                |_| ConversionError::new("documentation_score", "Invalid documentation score"),
            )?,
            recommendations,
            graded_at: value.graded_at,
            process_weight_permille: value
                .process_weight_permille
                .max(0)
                .try_into()
                .map_err(|_| ConversionError::new("process_weight_permille", "Invalid weight"))?,
            risk_weight_permille: value
                .risk_weight_permille
                .max(0)
                .try_into()
                .map_err(|_| ConversionError::new("risk_weight_permille", "Invalid weight"))?,
            execution_weight_permille: value
                .execution_weight_permille
                .max(0)
                .try_into()
                .map_err(|_| ConversionError::new("execution_weight_permille", "Invalid weight"))?,
            documentation_weight_permille: value
                .documentation_weight_permille
                .max(0)
                .try_into()
                .map_err(|_| {
                    ConversionError::new("documentation_weight_permille", "Invalid weight")
                })?,
        })
    }
}

impl IntoDomainModel<TradeGrade> for TradeGradeSQLite {
    fn into_domain_model(self) -> Result<TradeGrade, Box<dyn Error>> {
        self.try_into().map_err(Into::into)
    }
}

#[derive(Insertable)]
#[diesel(table_name = trade_grades)]
#[diesel(treat_none_as_null = true)]
struct NewTradeGrade {
    id: String,
    created_at: NaiveDateTime,
    updated_at: NaiveDateTime,
    deleted_at: Option<NaiveDateTime>,
    trade_id: String,
    overall_score: i32,
    overall_grade: String,
    process_score: i32,
    risk_score: i32,
    execution_score: i32,
    documentation_score: i32,
    recommendations: Option<String>,
    graded_at: NaiveDateTime,
    process_weight_permille: i32,
    risk_weight_permille: i32,
    execution_weight_permille: i32,
    documentation_weight_permille: i32,
}

#[cfg(test)]
mod tests {
    #![allow(clippy::indexing_slicing, clippy::too_many_lines)]

    use super::*;
    use crate::workers::{WorkerOrder, WorkerTrade, WorkerTradingVehicle};
    use diesel::Connection;
    use diesel_migrations::*;
    use model::{
        Currency, DraftTrade, OrderAction, OrderCategory, Status, TradeCategory,
        TradingVehicleCategory,
    };
    use rust_decimal_macros::dec;

    #[test]
    fn test_create_and_read_latest_trade_grade_roundtrip() {
        pub const MIGRATIONS: EmbeddedMigrations = embed_migrations!();

        let mut conn = SqliteConnection::establish(":memory:").unwrap();
        conn.run_pending_migrations(MIGRATIONS).unwrap();
        conn.begin_test_transaction().unwrap();

        let now = Utc::now().naive_utc();
        let account_id = Uuid::new_v4();

        let tv = WorkerTradingVehicle::create(
            &mut conn,
            "AAPL",
            Some("US0378331005"),
            &TradingVehicleCategory::Stock,
            "NASDAQ",
        )
        .unwrap();

        let stop = WorkerOrder::create(
            &mut conn,
            dec!(190),
            &Currency::USD,
            10,
            &OrderAction::Sell,
            &OrderCategory::Stop,
            &tv,
        )
        .unwrap();
        let entry = WorkerOrder::create(
            &mut conn,
            dec!(200),
            &Currency::USD,
            10,
            &OrderAction::Buy,
            &OrderCategory::Limit,
            &tv,
        )
        .unwrap();
        let target = WorkerOrder::create(
            &mut conn,
            dec!(220),
            &Currency::USD,
            10,
            &OrderAction::Sell,
            &OrderCategory::Limit,
            &tv,
        )
        .unwrap();

        let trade = WorkerTrade::create(
            &mut conn,
            DraftTrade {
                account: model::Account {
                    id: account_id,
                    ..Default::default()
                },
                trading_vehicle: tv,
                quantity: 10,
                currency: Currency::USD,
                category: TradeCategory::Long,
                thesis: None,
                sector: None,
                asset_class: None,
                context: None,
            },
            &stop,
            &entry,
            &target,
        )
        .unwrap();

        // Ensure trade exists for account join filters used by read_for_account_days.
        assert_eq!(trade.account_id, account_id);
        assert_eq!(trade.status, Status::New);

        let grade = TradeGrade {
            id: Uuid::new_v4(),
            created_at: now,
            updated_at: now,
            deleted_at: None,
            trade_id: trade.id,
            overall_score: 87,
            overall_grade: Grade::BPlus,
            process_score: 90,
            risk_score: 95,
            execution_score: 80,
            documentation_score: 75,
            recommendations: vec!["do_thing".to_string(), "do_other".to_string()],
            graded_at: now,
            process_weight_permille: 400,
            risk_weight_permille: 300,
            execution_weight_permille: 200,
            documentation_weight_permille: 100,
        };

        let created = WorkerTradeGrade::create(&mut conn, &grade).unwrap();
        assert_eq!(created.trade_id, trade.id);
        assert_eq!(created.overall_score, 87);
        assert_eq!(created.overall_grade, Grade::BPlus);
        assert_eq!(created.recommendations.len(), 2);

        let latest = WorkerTradeGrade::read_latest_for_trade(&mut conn, trade.id)
            .unwrap()
            .expect("latest must exist");
        assert_eq!(latest.id, created.id);
        assert_eq!(latest.recommendations, created.recommendations);

        let by_account =
            WorkerTradeGrade::read_for_account_days(&mut conn, account_id, 30).unwrap();
        assert_eq!(by_account.len(), 1);
        assert_eq!(by_account[0].id, created.id);
    }
}
