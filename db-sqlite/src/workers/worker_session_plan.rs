use crate::error::{ConversionError, IntoDomainModel, IntoDomainModels};
use crate::schema::session_plans;
use chrono::{NaiveDateTime, Utc};
use diesel::prelude::*;
use model::{
    format_session_setups, parse_session_setups, SessionPlan, SessionPlanClose, SessionRegime,
};
use std::error::Error;
use std::str::FromStr;
use tracing::error;
use uuid::Uuid;

/// Worker for handling plan-act-review session plan database operations.
#[derive(Debug)]
pub struct WorkerSessionPlan;

impl WorkerSessionPlan {
    /// Create a new open session plan.
    pub fn create(
        connection: &mut SqliteConnection,
        session_plan: &SessionPlan,
    ) -> Result<SessionPlan, Box<dyn Error>> {
        Self::validate_open_plan(session_plan)?;
        let permitted_setups = format_session_setups(&session_plan.permitted_setups)
            .map_err(|error| format!("invalid session plan permitted_setups: {error:?}"))?;

        let record = NewSessionPlan {
            id: session_plan.id.to_string(),
            created_at: session_plan.created_at,
            updated_at: session_plan.updated_at,
            deleted_at: session_plan.deleted_at,
            account_id: session_plan.account_id.to_string(),
            opened_at: session_plan.opened_at,
            closed_at: session_plan.closed_at,
            regime: session_plan.regime.to_string(),
            permitted_setups,
            max_positions: session_plan.max_positions,
            hypothesis: session_plan.hypothesis.clone(),
            success_criteria: session_plan.success_criteria.clone(),
            failure_criteria: session_plan.failure_criteria.clone(),
            session_grade: session_plan.session_grade.clone(),
            adherence_notes: session_plan.adherence_notes.clone(),
        };

        diesel::insert_into(session_plans::table)
            .values(&record)
            .get_result::<SessionPlanSQLite>(connection)
            .map_err(|error| {
                error!("Error creating session plan: {:?}", error);
                error
            })?
            .into_domain_model()
    }

    /// Close an open session plan with review data.
    pub fn close(
        connection: &mut SqliteConnection,
        close: &SessionPlanClose,
    ) -> Result<SessionPlan, Box<dyn Error>> {
        Self::validate_review_field("session_grade", &close.session_grade)?;
        Self::validate_review_field("adherence_notes", &close.adherence_notes)?;

        let current = Self::read_active_by_id(connection, close.session_plan_id)?
            .ok_or("session plan not found")?;
        if current.closed_at.is_some() {
            return Err("session plan is already closed".into());
        }
        if close.closed_at < current.opened_at {
            return Err("session plan closed_at cannot be before opened_at".into());
        }

        diesel::update(
            session_plans::table
                .filter(session_plans::id.eq(close.session_plan_id.to_string()))
                .filter(session_plans::deleted_at.is_null())
                .filter(session_plans::closed_at.is_null()),
        )
        .set((
            session_plans::closed_at.eq(Some(close.closed_at)),
            session_plans::updated_at.eq(Utc::now().naive_utc()),
            session_plans::session_grade.eq(close.session_grade.clone()),
            session_plans::adherence_notes.eq(close.adherence_notes.clone()),
        ))
        .get_result::<SessionPlanSQLite>(connection)
        .map_err(|error| {
            error!("Error closing session plan: {:?}", error);
            error
        })?
        .into_domain_model()
    }

    /// Read an account's open session plan, if present.
    pub fn read_open(
        connection: &mut SqliteConnection,
        account_id: Uuid,
    ) -> Result<Option<SessionPlan>, Box<dyn Error>> {
        let row = session_plans::table
            .filter(session_plans::deleted_at.is_null())
            .filter(session_plans::closed_at.is_null())
            .filter(session_plans::account_id.eq(account_id.to_string()))
            .order_by((session_plans::opened_at.desc(), session_plans::id.asc()))
            .first::<SessionPlanSQLite>(connection)
            .optional()
            .map_err(|error| {
                error!("Error reading open session plan: {:?}", error);
                error
            })?;

        match row {
            Some(row) => row.into_domain_model().map(Some),
            None => Ok(None),
        }
    }

    /// Read active session plans for an account within an inclusive opened-at period.
    pub fn read_for_account_in_period(
        connection: &mut SqliteConnection,
        account_id: Uuid,
        start_at: NaiveDateTime,
        end_at: NaiveDateTime,
    ) -> Result<Vec<SessionPlan>, Box<dyn Error>> {
        if end_at < start_at {
            return Err("session plan period end_at cannot be before start_at".into());
        }

        session_plans::table
            .filter(session_plans::deleted_at.is_null())
            .filter(session_plans::account_id.eq(account_id.to_string()))
            .filter(session_plans::opened_at.ge(start_at))
            .filter(session_plans::opened_at.le(end_at))
            .order_by((session_plans::opened_at.asc(), session_plans::id.asc()))
            .load::<SessionPlanSQLite>(connection)
            .map_err(|error| {
                error!("Error reading account session plans: {:?}", error);
                error
            })?
            .into_domain_models()
    }

    fn read_active_by_id(
        connection: &mut SqliteConnection,
        session_plan_id: Uuid,
    ) -> Result<Option<SessionPlan>, Box<dyn Error>> {
        let row = session_plans::table
            .filter(session_plans::id.eq(session_plan_id.to_string()))
            .filter(session_plans::deleted_at.is_null())
            .first::<SessionPlanSQLite>(connection)
            .optional()
            .map_err(|error| {
                error!("Error reading session plan by id: {:?}", error);
                error
            })?;

        match row {
            Some(row) => row.into_domain_model().map(Some),
            None => Ok(None),
        }
    }

    fn validate_open_plan(session_plan: &SessionPlan) -> Result<(), Box<dyn Error>> {
        if session_plan.closed_at.is_some() {
            return Err("new session plan cannot already be closed".into());
        }
        if session_plan.session_grade.is_some() || session_plan.adherence_notes.is_some() {
            return Err("new session plan cannot include review fields".into());
        }
        if session_plan.max_positions < 0 {
            return Err("session plan max_positions cannot be negative".into());
        }
        Self::validate_required_text("hypothesis", &session_plan.hypothesis)?;
        if session_plan.hypothesis.chars().count() > 500 {
            return Err("session plan hypothesis cannot exceed 500 characters".into());
        }
        Self::validate_required_text("success_criteria", &session_plan.success_criteria)?;
        Self::validate_required_text("failure_criteria", &session_plan.failure_criteria)?;
        Ok(())
    }

    fn validate_required_text(field: &str, value: &str) -> Result<(), Box<dyn Error>> {
        if value.trim().is_empty() {
            return Err(format!("session plan {field} cannot be empty").into());
        }
        Ok(())
    }

    fn validate_review_field(field: &str, value: &Option<String>) -> Result<(), Box<dyn Error>> {
        if value.as_deref().is_some_and(|text| text.trim().is_empty()) {
            return Err(format!("session plan {field} cannot be blank").into());
        }
        Ok(())
    }
}

#[derive(Debug, Queryable, Identifiable, Selectable)]
#[diesel(table_name = session_plans)]
struct SessionPlanSQLite {
    id: String,
    created_at: NaiveDateTime,
    updated_at: NaiveDateTime,
    deleted_at: Option<NaiveDateTime>,
    account_id: String,
    opened_at: NaiveDateTime,
    closed_at: Option<NaiveDateTime>,
    regime: String,
    permitted_setups: String,
    max_positions: i32,
    hypothesis: String,
    success_criteria: String,
    failure_criteria: String,
    session_grade: Option<String>,
    adherence_notes: Option<String>,
}

impl TryFrom<SessionPlanSQLite> for SessionPlan {
    type Error = ConversionError;

    fn try_from(value: SessionPlanSQLite) -> Result<Self, Self::Error> {
        let regime = SessionRegime::from_str(&value.regime)
            .map_err(|_| ConversionError::new("regime", "Failed to parse session regime"))?;
        let permitted_setups = parse_session_setups(&value.permitted_setups).map_err(|_| {
            ConversionError::new("permitted_setups", "Failed to parse permitted setups")
        })?;

        Ok(SessionPlan {
            id: Uuid::parse_str(&value.id)
                .map_err(|_| ConversionError::new("id", "Failed to parse session plan ID"))?,
            created_at: value.created_at,
            updated_at: value.updated_at,
            deleted_at: value.deleted_at,
            account_id: Uuid::parse_str(&value.account_id)
                .map_err(|_| ConversionError::new("account_id", "Failed to parse account ID"))?,
            opened_at: value.opened_at,
            closed_at: value.closed_at,
            regime,
            permitted_setups,
            max_positions: value.max_positions,
            hypothesis: value.hypothesis,
            success_criteria: value.success_criteria,
            failure_criteria: value.failure_criteria,
            session_grade: value.session_grade,
            adherence_notes: value.adherence_notes,
        })
    }
}

impl IntoDomainModel<SessionPlan> for SessionPlanSQLite {
    fn into_domain_model(self) -> Result<SessionPlan, Box<dyn Error>> {
        self.try_into().map_err(Into::into)
    }
}

#[derive(Insertable)]
#[diesel(table_name = session_plans)]
#[diesel(treat_none_as_null = true)]
struct NewSessionPlan {
    id: String,
    created_at: NaiveDateTime,
    updated_at: NaiveDateTime,
    deleted_at: Option<NaiveDateTime>,
    account_id: String,
    opened_at: NaiveDateTime,
    closed_at: Option<NaiveDateTime>,
    regime: String,
    permitted_setups: String,
    max_positions: i32,
    hypothesis: String,
    success_criteria: String,
    failure_criteria: String,
    session_grade: Option<String>,
    adherence_notes: Option<String>,
}

#[cfg(test)]
mod tests {
    #![allow(clippy::indexing_slicing, clippy::too_many_lines)]

    use super::*;
    use crate::schema::accounts;
    use diesel::Connection;
    use diesel_migrations::*;

    const MIGRATIONS: EmbeddedMigrations = embed_migrations!("./migrations");

    #[derive(Insertable)]
    #[diesel(table_name = accounts)]
    struct TestAccount {
        id: String,
        created_at: NaiveDateTime,
        updated_at: NaiveDateTime,
        deleted_at: Option<NaiveDateTime>,
        name: String,
        description: String,
        environment: String,
        taxes_percentage: String,
        earnings_percentage: String,
        account_type: String,
        parent_account_id: Option<String>,
        broker_kind: String,
        broker_account_id: Option<String>,
    }

    fn setup() -> SqliteConnection {
        let mut connection =
            SqliteConnection::establish(":memory:").expect("in-memory db should open");
        diesel::sql_query("PRAGMA foreign_keys = ON")
            .execute(&mut connection)
            .expect("foreign keys should enable");
        connection
            .run_pending_migrations(MIGRATIONS)
            .expect("migrations should run");
        connection
            .begin_test_transaction()
            .expect("test transaction should start");
        connection
    }

    fn add_days(value: NaiveDateTime, days: i64) -> NaiveDateTime {
        value
            .checked_add_signed(chrono::Duration::days(days))
            .expect("test timestamp should be representable")
    }

    fn create_account(connection: &mut SqliteConnection, name: &str) -> Uuid {
        let now = Utc::now().naive_utc();
        let id = Uuid::new_v4();
        let account = TestAccount {
            id: id.to_string(),
            created_at: now,
            updated_at: now,
            deleted_at: None,
            name: name.to_string(),
            description: "session plan test account".to_string(),
            environment: "paper".to_string(),
            taxes_percentage: "10".to_string(),
            earnings_percentage: "50".to_string(),
            account_type: "primary".to_string(),
            parent_account_id: None,
            broker_kind: "alpaca".to_string(),
            broker_account_id: None,
        };

        diesel::insert_into(accounts::table)
            .values(account)
            .execute(connection)
            .expect("account should insert");
        id
    }

    fn plan_for(account_id: Uuid, opened_at: NaiveDateTime) -> SessionPlan {
        SessionPlan {
            id: Uuid::new_v4(),
            created_at: opened_at,
            updated_at: opened_at,
            deleted_at: None,
            account_id,
            opened_at,
            closed_at: None,
            regime: SessionRegime::Normal,
            permitted_setups: vec!["opening range".to_string(), "pullback".to_string()],
            max_positions: 2,
            hypothesis: "follow only planned setups".to_string(),
            success_criteria: "take valid setups only".to_string(),
            failure_criteria: "force trades outside plan".to_string(),
            session_grade: None,
            adherence_notes: None,
        }
    }

    fn close_for(session_plan_id: Uuid, closed_at: NaiveDateTime) -> SessionPlanClose {
        SessionPlanClose {
            session_plan_id,
            closed_at,
            session_grade: Some("A".to_string()),
            adherence_notes: Some("followed the plan".to_string()),
        }
    }

    fn base_sqlite_row() -> SessionPlanSQLite {
        let now = Utc::now().naive_utc();
        SessionPlanSQLite {
            id: Uuid::new_v4().to_string(),
            created_at: now,
            updated_at: now,
            deleted_at: None,
            account_id: Uuid::new_v4().to_string(),
            opened_at: now,
            closed_at: None,
            regime: "normal".to_string(),
            permitted_setups: "opening range,pullback".to_string(),
            max_positions: 2,
            hypothesis: "hypothesis".to_string(),
            success_criteria: "success".to_string(),
            failure_criteria: "failure".to_string(),
            session_grade: None,
            adherence_notes: None,
        }
    }

    fn assert_conversion_error(row: SessionPlanSQLite, field: &str) {
        let error = SessionPlan::try_from(row).expect_err("corrupt row must fail conversion");
        assert!(
            error.to_string().contains(field),
            "expected conversion error for {field}, got {error}"
        );
    }

    #[test]
    fn create_read_close_and_reopen_session_plan_roundtrip() {
        let mut conn = setup();
        let account_id = create_account(&mut conn, "session-plan-roundtrip");
        let opened_at = Utc::now().naive_utc();

        let plan = WorkerSessionPlan::create(&mut conn, &plan_for(account_id, opened_at)).unwrap();
        assert_eq!(plan.regime, SessionRegime::Normal);
        assert_eq!(
            WorkerSessionPlan::read_open(&mut conn, account_id)
                .unwrap()
                .map(|entry| entry.id),
            Some(plan.id)
        );

        let duplicate_error =
            WorkerSessionPlan::create(&mut conn, &plan_for(account_id, add_days(opened_at, 1)))
                .expect_err("second open session should violate unique index");
        assert!(duplicate_error.to_string().contains("session_plans"));

        let closed =
            WorkerSessionPlan::close(&mut conn, &close_for(plan.id, add_days(opened_at, 1)))
                .unwrap();
        assert_eq!(closed.session_grade.as_deref(), Some("A"));
        assert_eq!(closed.adherence_notes.as_deref(), Some("followed the plan"));
        assert_eq!(
            WorkerSessionPlan::read_open(&mut conn, account_id)
                .unwrap()
                .map(|entry| entry.id),
            None
        );

        let reopened =
            WorkerSessionPlan::create(&mut conn, &plan_for(account_id, add_days(opened_at, 2)))
                .unwrap();
        assert_eq!(
            WorkerSessionPlan::read_open(&mut conn, account_id)
                .unwrap()
                .map(|entry| entry.id),
            Some(reopened.id)
        );
    }

    #[test]
    fn read_for_account_in_period_filters_account_window_and_soft_deletes() {
        let mut conn = setup();
        let account_id = create_account(&mut conn, "session-plan-period");
        let other_account_id = create_account(&mut conn, "session-plan-other");
        let now = Utc::now().naive_utc();

        let old = WorkerSessionPlan::create(&mut conn, &plan_for(account_id, add_days(now, -10)))
            .unwrap();
        WorkerSessionPlan::close(&mut conn, &close_for(old.id, add_days(now, -9))).unwrap();

        let kept = WorkerSessionPlan::create(&mut conn, &plan_for(account_id, now)).unwrap();
        WorkerSessionPlan::close(&mut conn, &close_for(kept.id, add_days(now, 1))).unwrap();

        let future =
            WorkerSessionPlan::create(&mut conn, &plan_for(account_id, add_days(now, 10))).unwrap();
        WorkerSessionPlan::close(&mut conn, &close_for(future.id, add_days(now, 11))).unwrap();

        let other = WorkerSessionPlan::create(&mut conn, &plan_for(other_account_id, now)).unwrap();
        WorkerSessionPlan::close(&mut conn, &close_for(other.id, add_days(now, 1))).unwrap();

        let deleted =
            WorkerSessionPlan::create(&mut conn, &plan_for(account_id, add_days(now, 2))).unwrap();
        diesel::update(session_plans::table.filter(session_plans::id.eq(deleted.id.to_string())))
            .set(session_plans::deleted_at.eq(Some(now)))
            .execute(&mut conn)
            .unwrap();

        let sessions = WorkerSessionPlan::read_for_account_in_period(
            &mut conn,
            account_id,
            add_days(now, -1),
            add_days(now, 3),
        )
        .unwrap();

        assert_eq!(
            sessions.iter().map(|entry| entry.id).collect::<Vec<_>>(),
            vec![kept.id]
        );
    }

    #[test]
    fn read_for_account_in_period_rejects_invalid_window() {
        let mut conn = setup();
        let now = Utc::now().naive_utc();

        let error = WorkerSessionPlan::read_for_account_in_period(
            &mut conn,
            Uuid::new_v4(),
            add_days(now, 1),
            now,
        )
        .expect_err("reversed period should fail");

        assert!(error.to_string().contains("end_at"));
    }

    #[test]
    fn create_and_close_validate_required_fields_and_foreign_keys() {
        let mut conn = setup();
        let account_id = create_account(&mut conn, "session-plan-validation");
        let now = Utc::now().naive_utc();
        let base = plan_for(account_id, now);

        let missing_account = WorkerSessionPlan::create(&mut conn, &plan_for(Uuid::new_v4(), now))
            .expect_err("missing account FK should fail");
        assert!(
            missing_account.to_string().contains("FOREIGN KEY")
                || missing_account.to_string().contains("constraint"),
            "{missing_account}"
        );

        let closed_create = WorkerSessionPlan::create(
            &mut conn,
            &SessionPlan {
                closed_at: Some(add_days(now, 1)),
                ..base.clone()
            },
        )
        .expect_err("new closed session should fail");
        assert!(closed_create
            .to_string()
            .contains("cannot already be closed"));

        let review_create = WorkerSessionPlan::create(
            &mut conn,
            &SessionPlan {
                session_grade: Some("A".to_string()),
                ..base.clone()
            },
        )
        .expect_err("new reviewed session should fail");
        assert!(review_create.to_string().contains("review fields"));

        let negative_max = WorkerSessionPlan::create(
            &mut conn,
            &SessionPlan {
                max_positions: -1,
                ..base.clone()
            },
        )
        .expect_err("negative max positions should fail");
        assert!(negative_max.to_string().contains("max_positions"));

        let blank_hypothesis = WorkerSessionPlan::create(
            &mut conn,
            &SessionPlan {
                hypothesis: " ".to_string(),
                ..base.clone()
            },
        )
        .expect_err("blank hypothesis should fail");
        assert!(blank_hypothesis.to_string().contains("hypothesis"));

        let long_hypothesis = WorkerSessionPlan::create(
            &mut conn,
            &SessionPlan {
                hypothesis: "x".repeat(501),
                ..base.clone()
            },
        )
        .expect_err("long hypothesis should fail");
        assert!(long_hypothesis.to_string().contains("500"));

        let bad_setup = WorkerSessionPlan::create(
            &mut conn,
            &SessionPlan {
                permitted_setups: vec!["break,out".to_string()],
                ..base.clone()
            },
        )
        .expect_err("comma setup should fail");
        assert!(bad_setup.to_string().contains("permitted_setups"));

        let created = WorkerSessionPlan::create(&mut conn, &base).unwrap();
        let blank_grade = WorkerSessionPlan::close(
            &mut conn,
            &SessionPlanClose {
                session_grade: Some(" ".to_string()),
                ..close_for(created.id, add_days(now, 1))
            },
        )
        .expect_err("blank grade should fail");
        assert!(blank_grade.to_string().contains("session_grade"));

        let before_open =
            WorkerSessionPlan::close(&mut conn, &close_for(created.id, add_days(now, -1)))
                .expect_err("close before open should fail");
        assert!(before_open.to_string().contains("before opened_at"));

        WorkerSessionPlan::close(&mut conn, &close_for(created.id, add_days(now, 1))).unwrap();
        let close_again =
            WorkerSessionPlan::close(&mut conn, &close_for(created.id, add_days(now, 2)))
                .expect_err("already closed session should fail");
        assert!(close_again.to_string().contains("already closed"));
    }

    #[test]
    fn database_constraints_enforce_immutability_and_review_timing() {
        let mut conn = setup();
        let account_id = create_account(&mut conn, "session-plan-constraints");
        let now = Utc::now().naive_utc();
        let plan = WorkerSessionPlan::create(&mut conn, &plan_for(account_id, now)).unwrap();

        let immutable_error =
            diesel::update(session_plans::table.filter(session_plans::id.eq(plan.id.to_string())))
                .set(session_plans::hypothesis.eq("changed"))
                .execute(&mut conn)
                .expect_err("immutable plan field update should fail");
        assert!(
            immutable_error.to_string().contains("immutable"),
            "{immutable_error}"
        );

        let review_error =
            diesel::update(session_plans::table.filter(session_plans::id.eq(plan.id.to_string())))
                .set(session_plans::session_grade.eq(Some("A".to_string())))
                .execute(&mut conn)
                .expect_err("review field update without close should fail");
        assert!(
            review_error.to_string().contains("review"),
            "{review_error}"
        );
    }

    #[test]
    fn session_plan_worker_reports_database_errors() {
        let mut conn = setup();
        diesel::sql_query("DROP TABLE session_plans")
            .execute(&mut conn)
            .expect("session_plans table should drop");

        let now = Utc::now().naive_utc();
        let plan = plan_for(Uuid::new_v4(), now);
        let create_error = WorkerSessionPlan::create(&mut conn, &plan)
            .expect_err("missing table should fail session plan create");
        assert!(create_error.to_string().contains("session_plans"));

        let read_open_error = WorkerSessionPlan::read_open(&mut conn, Uuid::new_v4())
            .expect_err("missing table should fail open session read");
        assert!(read_open_error.to_string().contains("session_plans"));

        let read_period_error =
            WorkerSessionPlan::read_for_account_in_period(&mut conn, Uuid::new_v4(), now, now)
                .expect_err("missing table should fail period session read");
        assert!(read_period_error.to_string().contains("session_plans"));
    }

    #[test]
    fn session_plan_sqlite_conversion_reports_corrupt_fields() {
        assert_conversion_error(
            SessionPlanSQLite {
                id: "not-a-uuid".to_string(),
                ..base_sqlite_row()
            },
            "id",
        );
        assert_conversion_error(
            SessionPlanSQLite {
                account_id: "not-a-uuid".to_string(),
                ..base_sqlite_row()
            },
            "account_id",
        );
        assert_conversion_error(
            SessionPlanSQLite {
                regime: "volatile".to_string(),
                ..base_sqlite_row()
            },
            "regime",
        );
        assert_conversion_error(
            SessionPlanSQLite {
                permitted_setups: "breakout,,pullback".to_string(),
                ..base_sqlite_row()
            },
            "permitted_setups",
        );
    }

    #[test]
    fn debug_representation_is_stable() {
        assert_eq!(format!("{WorkerSessionPlan:?}"), "WorkerSessionPlan");
    }
}
