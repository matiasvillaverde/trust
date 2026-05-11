use crate::error::{ConversionError, IntoDomainModel, IntoDomainModels};
use crate::schema::{mistakes, trades};
use chrono::NaiveDateTime;
use diesel::prelude::*;
use model::{format_munger_tendencies, parse_munger_tendencies, Mistake, MistakeErrorType};
use rust_decimal::Decimal;
use std::error::Error;
use std::str::FromStr;
use tracing::error;
use uuid::Uuid;

/// Worker for handling post-trade mistake database operations.
#[derive(Debug)]
pub struct WorkerMistake;

impl WorkerMistake {
    /// Create a new mistake.
    pub fn create(
        connection: &mut SqliteConnection,
        mistake: &Mistake,
    ) -> Result<Mistake, Box<dyn Error>> {
        if mistake.bias_tags.is_empty() {
            return Err("mistake bias_tags cannot be empty".into());
        }
        if mistake.lesson.trim().is_empty() {
            return Err("mistake lesson cannot be empty".into());
        }

        let record = NewMistake {
            id: mistake.id.to_string(),
            created_at: mistake.created_at,
            updated_at: mistake.updated_at,
            deleted_at: mistake.deleted_at,
            trade_id: mistake.trade_id.to_string(),
            bias_tags: format_munger_tendencies(&mistake.bias_tags),
            lollapalooza: mistake.lollapalooza,
            error_type: mistake.error_type.to_string(),
            rule_violated: mistake.rule_violated.clone(),
            counterfactual_r: mistake.counterfactual_r.to_string(),
            lesson: mistake.lesson.clone(),
        };

        diesel::insert_into(mistakes::table)
            .values(&record)
            .get_result::<MistakeSQLite>(connection)
            .map_err(|error| {
                error!("Error creating mistake: {:?}", error);
                error
            })?
            .into_domain_model()
    }

    /// Read active mistakes for a trade.
    pub fn read_for_trade(
        connection: &mut SqliteConnection,
        trade_id: Uuid,
    ) -> Result<Vec<Mistake>, Box<dyn Error>> {
        mistakes::table
            .filter(mistakes::deleted_at.is_null())
            .filter(mistakes::trade_id.eq(trade_id.to_string()))
            .order_by((mistakes::created_at.asc(), mistakes::id.asc()))
            .load::<MistakeSQLite>(connection)
            .map_err(|error| {
                error!("Error reading mistakes for trade: {:?}", error);
                error
            })?
            .into_domain_models()
    }

    /// Read active mistakes for an account within an inclusive creation-time period.
    pub fn read_for_account_in_period(
        connection: &mut SqliteConnection,
        account_id: Uuid,
        start_at: NaiveDateTime,
        end_at: NaiveDateTime,
    ) -> Result<Vec<Mistake>, Box<dyn Error>> {
        if end_at < start_at {
            return Err("mistake period end_at cannot be before start_at".into());
        }

        mistakes::table
            .inner_join(trades::table.on(trades::id.eq(mistakes::trade_id)))
            .select(MistakeSQLite::as_select())
            .filter(mistakes::deleted_at.is_null())
            .filter(trades::deleted_at.is_null())
            .filter(trades::account_id.eq(account_id.to_string()))
            .filter(mistakes::created_at.ge(start_at))
            .filter(mistakes::created_at.le(end_at))
            .order_by((mistakes::created_at.asc(), mistakes::id.asc()))
            .load::<MistakeSQLite>(connection)
            .map_err(|error| {
                error!("Error reading mistakes for account period: {:?}", error);
                error
            })?
            .into_domain_models()
    }
}

#[derive(Debug, Queryable, Identifiable, Selectable)]
#[diesel(table_name = mistakes)]
struct MistakeSQLite {
    id: String,
    created_at: NaiveDateTime,
    updated_at: NaiveDateTime,
    deleted_at: Option<NaiveDateTime>,
    trade_id: String,
    bias_tags: String,
    lollapalooza: bool,
    error_type: String,
    rule_violated: Option<String>,
    counterfactual_r: String,
    lesson: String,
}

impl TryFrom<MistakeSQLite> for Mistake {
    type Error = ConversionError;

    fn try_from(value: MistakeSQLite) -> Result<Self, Self::Error> {
        let bias_tags = parse_munger_tendencies(&value.bias_tags)
            .map_err(|_| ConversionError::new("bias_tags", "Failed to parse bias tags"))?;
        let error_type = MistakeErrorType::from_str(&value.error_type)
            .map_err(|_| ConversionError::new("error_type", "Failed to parse error type"))?;
        let counterfactual_r = Decimal::from_str(&value.counterfactual_r).map_err(|_| {
            ConversionError::new("counterfactual_r", "Failed to parse counterfactual R")
        })?;

        Ok(Mistake {
            id: Uuid::parse_str(&value.id)
                .map_err(|_| ConversionError::new("id", "Failed to parse mistake ID"))?,
            created_at: value.created_at,
            updated_at: value.updated_at,
            deleted_at: value.deleted_at,
            trade_id: Uuid::parse_str(&value.trade_id)
                .map_err(|_| ConversionError::new("trade_id", "Failed to parse trade ID"))?,
            bias_tags,
            lollapalooza: value.lollapalooza,
            error_type,
            rule_violated: value.rule_violated,
            counterfactual_r,
            lesson: value.lesson,
        })
    }
}

impl IntoDomainModel<Mistake> for MistakeSQLite {
    fn into_domain_model(self) -> Result<Mistake, Box<dyn Error>> {
        self.try_into().map_err(Into::into)
    }
}

#[derive(Insertable)]
#[diesel(table_name = mistakes)]
#[diesel(treat_none_as_null = true)]
struct NewMistake {
    id: String,
    created_at: NaiveDateTime,
    updated_at: NaiveDateTime,
    deleted_at: Option<NaiveDateTime>,
    trade_id: String,
    bias_tags: String,
    lollapalooza: bool,
    error_type: String,
    rule_violated: Option<String>,
    counterfactual_r: String,
    lesson: String,
}

#[cfg(test)]
mod tests {
    #![allow(clippy::indexing_slicing, clippy::too_many_lines)]

    use super::*;
    use crate::schema::accounts;
    use crate::workers::{WorkerOrder, WorkerTrade, WorkerTradingVehicle};
    use chrono::Utc;
    use diesel::Connection;
    use diesel_migrations::*;
    use model::{
        Currency, DraftTrade, MungerTendency, OrderAction, OrderCategory, Status, TradeCategory,
        TradingVehicleCategory,
    };
    use rust_decimal_macros::dec;

    pub const MIGRATIONS: EmbeddedMigrations = embed_migrations!();

    fn setup_connection() -> SqliteConnection {
        let mut conn = SqliteConnection::establish(":memory:").unwrap();
        conn.run_pending_migrations(MIGRATIONS).unwrap();
        diesel::sql_query("PRAGMA foreign_keys=ON;")
            .execute(&mut conn)
            .unwrap();
        conn.begin_test_transaction().unwrap();
        conn
    }

    fn ensure_account(conn: &mut SqliteConnection, account_id: Uuid) {
        let now = Utc::now().naive_utc();
        diesel::insert_or_ignore_into(accounts::table)
            .values((
                accounts::id.eq(account_id.to_string()),
                accounts::created_at.eq(now),
                accounts::updated_at.eq(now),
                accounts::deleted_at.eq(Option::<NaiveDateTime>::None),
                accounts::name.eq(format!("account-{account_id}")),
                accounts::description.eq("fixture account"),
                accounts::environment.eq("paper"),
                accounts::taxes_percentage.eq("0"),
                accounts::earnings_percentage.eq("0"),
                accounts::account_type.eq("primary"),
                accounts::parent_account_id.eq(Option::<String>::None),
                accounts::broker_kind.eq("alpaca"),
                accounts::broker_account_id.eq(Option::<String>::None),
            ))
            .execute(conn)
            .unwrap();
    }

    fn create_trade(conn: &mut SqliteConnection, account_id: Uuid) -> model::Trade {
        ensure_account(conn, account_id);
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
            10,
            &OrderAction::Sell,
            &OrderCategory::Stop,
            &tv,
        )
        .unwrap();
        let entry = WorkerOrder::create(
            conn,
            dec!(200),
            &Currency::USD,
            10,
            &OrderAction::Buy,
            &OrderCategory::Limit,
            &tv,
        )
        .unwrap();
        let target = WorkerOrder::create(
            conn,
            dec!(220),
            &Currency::USD,
            10,
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
        .unwrap()
    }

    fn mistake_for(trade_id: Uuid, created_at: NaiveDateTime) -> Mistake {
        Mistake {
            id: Uuid::new_v4(),
            created_at,
            updated_at: created_at,
            deleted_at: None,
            trade_id,
            bias_tags: vec![
                MungerTendency::InconsistencyAvoidance,
                MungerTendency::DeprivalSuperreaction,
            ],
            lollapalooza: true,
            error_type: MistakeErrorType::Commission,
            rule_violated: Some("move_stop_only_to_reduce_risk".to_string()),
            counterfactual_r: dec!(1.75),
            lesson: "Pre-commit stop movement criteria before entry.".to_string(),
        }
    }

    fn base_sqlite_row() -> MistakeSQLite {
        let now = Utc::now().naive_utc();
        MistakeSQLite {
            id: Uuid::new_v4().to_string(),
            created_at: now,
            updated_at: now,
            deleted_at: None,
            trade_id: Uuid::new_v4().to_string(),
            bias_tags: "5,14".to_string(),
            lollapalooza: true,
            error_type: "commission".to_string(),
            rule_violated: Some("risk_rule".to_string()),
            counterfactual_r: "1.25".to_string(),
            lesson: "Follow the rule.".to_string(),
        }
    }

    fn assert_conversion_error(row: MistakeSQLite, field: &str) {
        let error = Mistake::try_from(row).expect_err("corrupt row must fail conversion");
        assert!(error.to_string().contains(field));
    }

    #[test]
    fn create_and_read_mistakes_for_trade_roundtrip() {
        let mut conn = setup_connection();
        let account_id = Uuid::new_v4();
        let trade = create_trade(&mut conn, account_id);
        let now = Utc::now().naive_utc();
        let later = mistake_for(trade.id, now);
        let earlier = Mistake {
            error_type: MistakeErrorType::Omission,
            rule_violated: None,
            counterfactual_r: dec!(-0.50),
            lesson: "Take the qualified setup when risk is valid.".to_string(),
            ..mistake_for(trade.id, now - chrono::Duration::days(1))
        };

        assert_eq!(trade.account_id, account_id);
        assert_eq!(trade.status, Status::New);

        let created_later = WorkerMistake::create(&mut conn, &later).unwrap();
        let created_earlier = WorkerMistake::create(&mut conn, &earlier).unwrap();

        assert_eq!(created_later.trade_id, trade.id);
        assert_eq!(created_later.bias_tags, later.bias_tags);
        assert_eq!(created_later.counterfactual_r, dec!(1.75));
        assert_eq!(created_later.error_type, MistakeErrorType::Commission);

        let mistakes = WorkerMistake::read_for_trade(&mut conn, trade.id).unwrap();
        assert_eq!(
            mistakes
                .iter()
                .map(|mistake| mistake.id)
                .collect::<Vec<_>>(),
            vec![created_earlier.id, created_later.id]
        );
        assert_eq!(
            mistakes.first().map(|mistake| mistake.error_type),
            Some(MistakeErrorType::Omission)
        );
    }

    #[test]
    fn read_for_account_in_period_filters_account_window_and_soft_deletes() {
        let mut conn = setup_connection();
        let account_id = Uuid::new_v4();
        let other_account_id = Uuid::new_v4();
        let trade = create_trade(&mut conn, account_id);
        let other_trade = create_trade(&mut conn, other_account_id);
        let now = Utc::now().naive_utc();
        let start = now - chrono::Duration::days(3);
        let end = now + chrono::Duration::days(3);

        let kept = WorkerMistake::create(&mut conn, &mistake_for(trade.id, now)).unwrap();
        let old = WorkerMistake::create(
            &mut conn,
            &mistake_for(trade.id, now - chrono::Duration::days(10)),
        )
        .unwrap();
        let future = WorkerMistake::create(
            &mut conn,
            &mistake_for(trade.id, now + chrono::Duration::days(10)),
        )
        .unwrap();
        let other = WorkerMistake::create(&mut conn, &mistake_for(other_trade.id, now)).unwrap();
        let deleted = WorkerMistake::create(&mut conn, &mistake_for(trade.id, now)).unwrap();

        diesel::update(mistakes::table.filter(mistakes::id.eq(deleted.id.to_string())))
            .set(mistakes::deleted_at.eq(Some(now)))
            .execute(&mut conn)
            .unwrap();

        let mistakes =
            WorkerMistake::read_for_account_in_period(&mut conn, account_id, start, end).unwrap();

        assert_eq!(
            mistakes
                .iter()
                .map(|mistake| mistake.id)
                .collect::<Vec<_>>(),
            vec![kept.id]
        );
        assert!(!mistakes.iter().any(|mistake| mistake.id == old.id));
        assert!(!mistakes.iter().any(|mistake| mistake.id == future.id));
        assert!(!mistakes.iter().any(|mistake| mistake.id == other.id));
        assert!(!mistakes.iter().any(|mistake| mistake.id == deleted.id));
    }

    #[test]
    fn read_for_account_in_period_rejects_invalid_window() {
        let mut conn = setup_connection();
        let now = Utc::now().naive_utc();

        let error = WorkerMistake::read_for_account_in_period(
            &mut conn,
            Uuid::new_v4(),
            now,
            now - chrono::Duration::seconds(1),
        )
        .expect_err("inverted period should fail before querying");

        assert!(error.to_string().contains("end_at"));
    }

    #[test]
    fn create_rejects_missing_trade_foreign_key_and_invalid_required_fields() {
        let mut conn = setup_connection();
        let now = Utc::now().naive_utc();
        let base = mistake_for(Uuid::new_v4(), now);

        let fk_error =
            WorkerMistake::create(&mut conn, &base).expect_err("missing trade FK should fail");
        assert!(fk_error.to_string().contains("FOREIGN KEY"));

        let empty_tags = WorkerMistake::create(
            &mut conn,
            &Mistake {
                bias_tags: Vec::new(),
                ..base.clone()
            },
        )
        .expect_err("empty bias tags should fail");
        let empty_lesson = WorkerMistake::create(
            &mut conn,
            &Mistake {
                lesson: "   ".to_string(),
                ..base
            },
        )
        .expect_err("empty lesson should fail");

        assert!(empty_tags.to_string().contains("bias_tags"));
        assert!(empty_lesson.to_string().contains("lesson"));
    }

    #[test]
    fn mistake_worker_reports_database_errors() {
        let mut conn = setup_connection();
        diesel::sql_query("DROP TABLE mistakes")
            .execute(&mut conn)
            .expect("mistakes table should drop");
        let mistake = mistake_for(Uuid::new_v4(), Utc::now().naive_utc());

        let create_error = WorkerMistake::create(&mut conn, &mistake)
            .expect_err("missing table should fail mistake create");
        assert!(create_error.to_string().contains("mistakes"));

        let trade_error = WorkerMistake::read_for_trade(&mut conn, Uuid::new_v4())
            .expect_err("missing table should fail trade mistake read");
        assert!(trade_error.to_string().contains("mistakes"));

        let now = Utc::now().naive_utc();
        let account_error =
            WorkerMistake::read_for_account_in_period(&mut conn, Uuid::new_v4(), now, now)
                .expect_err("missing table should fail account mistake read");
        assert!(account_error.to_string().contains("mistakes"));
    }

    #[test]
    fn mistake_sqlite_conversion_reports_corrupt_fields() {
        assert_conversion_error(
            MistakeSQLite {
                id: "not-a-uuid".to_string(),
                ..base_sqlite_row()
            },
            "id",
        );
        assert_conversion_error(
            MistakeSQLite {
                trade_id: "not-a-uuid".to_string(),
                ..base_sqlite_row()
            },
            "trade_id",
        );
        assert_conversion_error(
            MistakeSQLite {
                bias_tags: "5,2".to_string(),
                ..base_sqlite_row()
            },
            "bias_tags",
        );
        assert_conversion_error(
            MistakeSQLite {
                error_type: "accident".to_string(),
                ..base_sqlite_row()
            },
            "error_type",
        );
        assert_conversion_error(
            MistakeSQLite {
                counterfactual_r: "not-decimal".to_string(),
                ..base_sqlite_row()
            },
            "counterfactual_r",
        );
    }

    #[test]
    fn debug_representation_is_stable() {
        assert_eq!(format!("{WorkerMistake:?}"), "WorkerMistake");
    }
}
