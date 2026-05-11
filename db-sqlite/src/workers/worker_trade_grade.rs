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

    pub const MIGRATIONS: EmbeddedMigrations = embed_migrations!();

    fn setup_connection() -> SqliteConnection {
        let mut conn = SqliteConnection::establish(":memory:").unwrap();
        conn.run_pending_migrations(MIGRATIONS).unwrap();
        conn.begin_test_transaction().unwrap();
        conn
    }

    fn create_trade(conn: &mut SqliteConnection, account_id: Uuid) -> model::Trade {
        let symbol = format!("T{}", Uuid::new_v4().simple());
        let tv = WorkerTradingVehicle::create(
            conn,
            &symbol,
            None,
            &TradingVehicleCategory::Stock,
            "NASDAQ",
        )
        .unwrap();

        let stop = WorkerOrder::create(
            conn,
            dec!(190),
            &Currency::USD,
            dec!(10),
            &OrderAction::Sell,
            &OrderCategory::Stop,
            &tv,
        )
        .unwrap();
        let entry = WorkerOrder::create(
            conn,
            dec!(200),
            &Currency::USD,
            dec!(10),
            &OrderAction::Buy,
            &OrderCategory::Limit,
            &tv,
        )
        .unwrap();
        let target = WorkerOrder::create(
            conn,
            dec!(220),
            &Currency::USD,
            dec!(10),
            &OrderAction::Sell,
            &OrderCategory::Limit,
            &tv,
        )
        .unwrap();

        WorkerTrade::create(
            conn,
            DraftTrade {
                account: model::Account {
                    id: account_id,
                    ..Default::default()
                },
                trading_vehicle: tv,
                quantity: 10.into(),
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
        .unwrap()
    }

    fn grade_for(trade_id: Uuid, graded_at: NaiveDateTime) -> TradeGrade {
        TradeGrade {
            id: Uuid::new_v4(),
            created_at: graded_at,
            updated_at: graded_at,
            deleted_at: None,
            trade_id,
            overall_score: 87,
            overall_grade: Grade::BPlus,
            process_score: 90,
            risk_score: 95,
            execution_score: 80,
            documentation_score: 75,
            recommendations: vec!["do_thing".to_string(), "do_other".to_string()],
            graded_at,
            process_weight_permille: 400,
            risk_weight_permille: 300,
            execution_weight_permille: 200,
            documentation_weight_permille: 100,
        }
    }

    fn base_sqlite_row() -> TradeGradeSQLite {
        let now = Utc::now().naive_utc();
        TradeGradeSQLite {
            id: Uuid::new_v4().to_string(),
            created_at: now,
            updated_at: now,
            deleted_at: None,
            trade_id: Uuid::new_v4().to_string(),
            overall_score: 87,
            overall_grade: "B+".to_string(),
            process_score: 90,
            risk_score: 95,
            execution_score: 80,
            documentation_score: 75,
            recommendations: Some(r#"["keep a tighter journal"]"#.to_string()),
            graded_at: now,
            process_weight_permille: 400,
            risk_weight_permille: 300,
            execution_weight_permille: 200,
            documentation_weight_permille: 100,
        }
    }

    fn assert_conversion_error(row: TradeGradeSQLite, field: &str) {
        let error = TradeGrade::try_from(row).expect_err("corrupt row must fail conversion");
        assert!(error.to_string().contains(field));
    }

    #[test]
    fn test_create_and_read_latest_trade_grade_roundtrip() {
        let mut conn = setup_connection();
        let now = Utc::now().naive_utc();
        let account_id = Uuid::new_v4();
        let trade = create_trade(&mut conn, account_id);

        // Ensure trade exists for account join filters used by read_for_account_days.
        assert_eq!(trade.account_id, account_id);
        assert_eq!(trade.status, Status::New);

        let grade = grade_for(trade.id, now);

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

    #[test]
    fn latest_grade_prefers_newest_active_grade_and_ignores_soft_deleted_rows() {
        let mut conn = setup_connection();
        let account_id = Uuid::new_v4();
        let trade = create_trade(&mut conn, account_id);
        let now = Utc::now().naive_utc();
        let older = WorkerTradeGrade::create(
            &mut conn,
            &TradeGrade {
                overall_score: 73,
                overall_grade: Grade::C,
                recommendations: Vec::new(),
                ..grade_for(trade.id, now - Duration::days(2))
            },
        )
        .unwrap();
        let newer = WorkerTradeGrade::create(
            &mut conn,
            &TradeGrade {
                overall_score: 97,
                overall_grade: Grade::APlus,
                ..grade_for(trade.id, now)
            },
        )
        .unwrap();

        let latest = WorkerTradeGrade::read_latest_for_trade(&mut conn, trade.id)
            .unwrap()
            .expect("latest active grade");
        assert_eq!(latest.id, newer.id);
        assert_eq!(latest.overall_grade, Grade::APlus);

        diesel::update(trade_grades::table.filter(trade_grades::id.eq(newer.id.to_string())))
            .set(trade_grades::deleted_at.eq(Some(now)))
            .execute(&mut conn)
            .unwrap();

        let latest = WorkerTradeGrade::read_latest_for_trade(&mut conn, trade.id)
            .unwrap()
            .expect("older active grade should remain");
        assert_eq!(latest.id, older.id);
        assert!(latest.recommendations.is_empty());
    }

    #[test]
    fn latest_grade_returns_none_when_trade_has_no_active_grade() {
        let mut conn = setup_connection();
        let trade_id = Uuid::new_v4();

        let latest = WorkerTradeGrade::read_latest_for_trade(&mut conn, trade_id).unwrap();

        assert_eq!(latest, None);
    }

    #[test]
    fn read_for_account_days_rejects_unrepresentable_window() {
        let mut conn = setup_connection();
        let error = WorkerTradeGrade::read_for_account_days(&mut conn, Uuid::new_v4(), u32::MAX)
            .expect_err("oversized day window should fail before querying");

        assert!(error.to_string().contains("Invalid days window"));
    }

    #[test]
    fn debug_representation_is_stable() {
        assert_eq!(format!("{WorkerTradeGrade:?}"), "WorkerTradeGrade");
    }

    #[test]
    fn trade_grade_worker_reports_database_errors() {
        let mut conn = setup_connection();
        diesel::sql_query("DROP TABLE trade_grades")
            .execute(&mut conn)
            .expect("trade_grades table should drop");
        let trade_id = Uuid::new_v4();

        let create_error =
            WorkerTradeGrade::create(&mut conn, &grade_for(trade_id, Utc::now().naive_utc()))
                .expect_err("missing table should fail trade grade create");
        assert!(create_error.to_string().contains("trade_grades"));

        let latest_error = WorkerTradeGrade::read_latest_for_trade(&mut conn, trade_id)
            .expect_err("missing table should fail latest grade read");
        assert!(latest_error.to_string().contains("trade_grades"));

        let account_error = WorkerTradeGrade::read_for_account_days(&mut conn, Uuid::new_v4(), 30)
            .expect_err("missing table should fail account grade read");
        assert!(account_error.to_string().contains("trade_grades"));
    }

    #[test]
    fn read_for_account_days_filters_account_window_and_soft_deletes_then_sorts_ascending() {
        let mut conn = setup_connection();
        let account_id = Uuid::new_v4();
        let other_account_id = Uuid::new_v4();
        let now = Utc::now().naive_utc();
        let older_trade = create_trade(&mut conn, account_id);
        let newer_trade = create_trade(&mut conn, account_id);
        let old_trade = create_trade(&mut conn, account_id);
        let other_trade = create_trade(&mut conn, other_account_id);
        let soft_deleted_grade_trade = create_trade(&mut conn, account_id);
        let soft_deleted_trade = create_trade(&mut conn, account_id);

        let older = WorkerTradeGrade::create(
            &mut conn,
            &grade_for(older_trade.id, now - Duration::days(3)),
        )
        .unwrap();
        let newer = WorkerTradeGrade::create(&mut conn, &grade_for(newer_trade.id, now)).unwrap();
        let old = WorkerTradeGrade::create(
            &mut conn,
            &grade_for(old_trade.id, now - Duration::days(40)),
        )
        .unwrap();
        let other = WorkerTradeGrade::create(&mut conn, &grade_for(other_trade.id, now)).unwrap();
        let deleted_grade =
            WorkerTradeGrade::create(&mut conn, &grade_for(soft_deleted_grade_trade.id, now))
                .unwrap();
        let deleted_trade =
            WorkerTradeGrade::create(&mut conn, &grade_for(soft_deleted_trade.id, now)).unwrap();

        diesel::update(
            trade_grades::table.filter(trade_grades::id.eq(deleted_grade.id.to_string())),
        )
        .set(trade_grades::deleted_at.eq(Some(now)))
        .execute(&mut conn)
        .unwrap();
        diesel::update(trades::table.filter(trades::id.eq(soft_deleted_trade.id.to_string())))
            .set(trades::deleted_at.eq(Some(now)))
            .execute(&mut conn)
            .unwrap();

        let by_account =
            WorkerTradeGrade::read_for_account_days(&mut conn, account_id, 30).unwrap();

        assert_eq!(
            by_account.iter().map(|grade| grade.id).collect::<Vec<_>>(),
            vec![older.id, newer.id]
        );
        assert!(!by_account.iter().any(|grade| grade.id == old.id));
        assert!(!by_account.iter().any(|grade| grade.id == other.id));
        assert!(!by_account.iter().any(|grade| grade.id == deleted_grade.id));
        assert!(!by_account.iter().any(|grade| grade.id == deleted_trade.id));
    }

    #[test]
    fn trade_grade_sqlite_conversion_clamps_scores_and_negative_weights() {
        let row = TradeGradeSQLite {
            overall_score: 120,
            process_score: -1,
            risk_score: 101,
            execution_score: -10,
            documentation_score: 42,
            process_weight_permille: -1,
            risk_weight_permille: -300,
            ..base_sqlite_row()
        };

        let grade = TradeGrade::try_from(row).unwrap();

        assert_eq!(grade.overall_score, 100);
        assert_eq!(grade.process_score, 0);
        assert_eq!(grade.risk_score, 100);
        assert_eq!(grade.execution_score, 0);
        assert_eq!(grade.documentation_score, 42);
        assert_eq!(grade.process_weight_permille, 0);
        assert_eq!(grade.risk_weight_permille, 0);
    }

    #[test]
    fn trade_grade_sqlite_conversion_reports_corrupt_fields() {
        assert_conversion_error(
            TradeGradeSQLite {
                id: "not-a-uuid".to_string(),
                ..base_sqlite_row()
            },
            "id",
        );
        assert_conversion_error(
            TradeGradeSQLite {
                trade_id: "not-a-uuid".to_string(),
                ..base_sqlite_row()
            },
            "trade_id",
        );
        assert_conversion_error(
            TradeGradeSQLite {
                overall_grade: "Z".to_string(),
                ..base_sqlite_row()
            },
            "overall_grade",
        );
        assert_conversion_error(
            TradeGradeSQLite {
                recommendations: Some("{not-json".to_string()),
                ..base_sqlite_row()
            },
            "recommendations",
        );
        assert_conversion_error(
            TradeGradeSQLite {
                process_weight_permille: i32::MAX,
                ..base_sqlite_row()
            },
            "process_weight_permille",
        );
        assert_conversion_error(
            TradeGradeSQLite {
                risk_weight_permille: i32::MAX,
                ..base_sqlite_row()
            },
            "risk_weight_permille",
        );
        assert_conversion_error(
            TradeGradeSQLite {
                execution_weight_permille: i32::MAX,
                ..base_sqlite_row()
            },
            "execution_weight_permille",
        );
        assert_conversion_error(
            TradeGradeSQLite {
                documentation_weight_permille: i32::MAX,
                ..base_sqlite_row()
            },
            "documentation_weight_permille",
        );
    }

    #[test]
    fn read_latest_surfaces_corrupt_row_id() {
        let mut conn = setup_connection();
        let account_id = Uuid::new_v4();
        let trade = create_trade(&mut conn, account_id);
        let now = Utc::now().naive_utc();

        diesel::insert_into(trade_grades::table)
            .values(NewTradeGrade {
                id: "not-a-uuid".to_string(),
                created_at: now,
                updated_at: now,
                deleted_at: None,
                trade_id: trade.id.to_string(),
                overall_score: 87,
                overall_grade: Grade::BPlus.to_string(),
                process_score: 90,
                risk_score: 95,
                execution_score: 80,
                documentation_score: 75,
                recommendations: None,
                graded_at: now,
                process_weight_permille: 400,
                risk_weight_permille: 300,
                execution_weight_permille: 200,
                documentation_weight_permille: 100,
            })
            .execute(&mut conn)
            .expect("corrupt trade grade row should insert for conversion test");

        let error = WorkerTradeGrade::read_latest_for_trade(&mut conn, trade.id)
            .expect_err("corrupt trade grade row should fail read conversion");

        assert!(error.to_string().contains("id"));
    }
}
