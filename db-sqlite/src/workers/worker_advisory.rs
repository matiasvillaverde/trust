use crate::error::ConversionError;
use chrono::Utc;
use diesel::prelude::*;
use diesel::sql_query;
use diesel::sql_types::Text;
use model::{AdvisoryRead, AdvisoryThresholds, AdvisoryWrite};
use rust_decimal::Decimal;
use std::error::Error;
use std::io;
use std::str::FromStr;
use std::sync::Arc;
use std::sync::Mutex;
use tracing::error;
use uuid::Uuid;

/// Database worker for advisory threshold configuration
pub struct AdvisoryDB {
    pub connection: Arc<Mutex<SqliteConnection>>,
}

#[derive(Debug, QueryableByName)]
struct AdvisoryThresholdRow {
    #[diesel(sql_type = Text)]
    sector_limit_pct: String,
    #[diesel(sql_type = Text)]
    asset_class_limit_pct: String,
    #[diesel(sql_type = Text)]
    single_position_limit_pct: String,
}

impl AdvisoryRead for AdvisoryDB {
    fn advisory_thresholds_for_account(
        &mut self,
        account_id: Uuid,
    ) -> Result<Option<AdvisoryThresholds>, Box<dyn Error>> {
        let rows: Vec<AdvisoryThresholdRow> = sql_query(
            "SELECT sector_limit_pct, asset_class_limit_pct, single_position_limit_pct \
            FROM advisory_thresholds WHERE account_id = ?1",
        )
        .bind::<Text, _>(account_id.to_string())
        .load(&mut *self.connection.lock().map_err(|error| {
            io::Error::other(format!("Failed to acquire connection lock: {error}"))
        })?)
        .map_err(|error| {
            error!("Error reading advisory thresholds: {:?}", error);
            error
        })?;

        let Some(row) = rows.into_iter().next() else {
            return Ok(None);
        };

        Ok(Some((
            parse_decimal(&row.sector_limit_pct, "sector_limit_pct")?,
            parse_decimal(&row.asset_class_limit_pct, "asset_class_limit_pct")?,
            parse_decimal(&row.single_position_limit_pct, "single_position_limit_pct")?,
        )))
    }
}

impl AdvisoryWrite for AdvisoryDB {
    fn upsert_advisory_thresholds(
        &mut self,
        account_id: Uuid,
        sector_limit_pct: Decimal,
        asset_class_limit_pct: Decimal,
        single_position_limit_pct: Decimal,
    ) -> Result<(), Box<dyn Error>> {
        let now = Utc::now().naive_utc().to_string();
        let id = Uuid::new_v4().to_string();
        let account_id = account_id.to_string();
        let sector_limit_pct = sector_limit_pct.to_string();
        let asset_class_limit_pct = asset_class_limit_pct.to_string();
        let single_position_limit_pct = single_position_limit_pct.to_string();

        sql_query(
            "INSERT INTO advisory_thresholds \
            (id, created_at, updated_at, account_id, sector_limit_pct, asset_class_limit_pct, single_position_limit_pct) \
            VALUES (?1, ?2, ?2, ?3, ?4, ?5, ?6) \
            ON CONFLICT(account_id) DO UPDATE SET \
                updated_at = excluded.updated_at, \
                sector_limit_pct = excluded.sector_limit_pct, \
                asset_class_limit_pct = excluded.asset_class_limit_pct, \
                single_position_limit_pct = excluded.single_position_limit_pct",
        )
        .bind::<Text, _>(id)
        .bind::<Text, _>(now.clone())
        .bind::<Text, _>(account_id)
        .bind::<Text, _>(sector_limit_pct)
        .bind::<Text, _>(asset_class_limit_pct)
        .bind::<Text, _>(single_position_limit_pct)
        .execute(&mut *self.connection.lock().map_err(|error| {
            io::Error::other(format!("Failed to acquire connection lock: {error}"))
        })?)
        .map_err(|error| {
            error!("Error upserting advisory thresholds: {:?}", error);
            error
        })?;

        Ok(())
    }
}

fn parse_decimal(value: &str, field: &str) -> Result<Decimal, Box<dyn Error>> {
    Decimal::from_str(value).map_err(|error| {
        Box::new(ConversionError::new(
            field,
            format!("Invalid advisory threshold decimal value in database: {error}"),
        )) as Box<dyn Error>
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use diesel::sql_types::BigInt;
    use diesel_migrations::*;
    use model::DatabaseFactory;
    use rust_decimal_macros::dec;
    use std::panic::{catch_unwind, resume_unwind, AssertUnwindSafe};
    use std::sync::{Arc, Mutex};

    pub const MIGRATIONS: EmbeddedMigrations = embed_migrations!();

    fn establish_connection() -> SqliteConnection {
        let mut connection = SqliteConnection::establish(":memory:").unwrap();
        connection.run_pending_migrations(MIGRATIONS).unwrap();
        connection.begin_test_transaction().unwrap();
        connection
    }

    fn create_factory() -> (crate::SqliteDatabase, Arc<Mutex<SqliteConnection>>) {
        let connection = Arc::new(Mutex::new(establish_connection()));
        (
            crate::SqliteDatabase::new_from(connection.clone()),
            connection,
        )
    }

    #[derive(Debug, QueryableByName)]
    struct CountRow {
        #[diesel(sql_type = BigInt)]
        count: i64,
    }

    fn insert_threshold_row(
        shared_connection: &Arc<Mutex<SqliteConnection>>,
        account_id: Uuid,
        sector_limit_pct: &str,
        asset_class_limit_pct: &str,
        single_position_limit_pct: &str,
    ) {
        let id = Uuid::new_v4();
        let now = chrono::Utc::now().naive_utc().to_string();

        sql_query(
            "INSERT INTO advisory_thresholds \
            (id, created_at, updated_at, account_id, sector_limit_pct, asset_class_limit_pct, single_position_limit_pct) \
            VALUES (?1, ?2, ?2, ?3, ?4, ?5, ?6)",
        )
        .bind::<Text, _>(id.to_string())
        .bind::<Text, _>(now)
        .bind::<Text, _>(account_id.to_string())
        .bind::<Text, _>(sector_limit_pct)
        .bind::<Text, _>(asset_class_limit_pct)
        .bind::<Text, _>(single_position_limit_pct)
        .execute(
            &mut *shared_connection
                .lock()
                .expect("Failed to acquire connection lock"),
        )
        .expect("insert advisory threshold row");
    }

    fn advisory_row_count(
        shared_connection: &Arc<Mutex<SqliteConnection>>,
        account_id: Uuid,
    ) -> i64 {
        sql_query("SELECT COUNT(*) AS count FROM advisory_thresholds WHERE account_id = ?1")
            .bind::<Text, _>(account_id.to_string())
            .load::<CountRow>(
                &mut *shared_connection
                    .lock()
                    .expect("Failed to acquire connection lock"),
            )
            .expect("count advisory threshold rows")
            .into_iter()
            .next()
            .expect("count row")
            .count
    }

    fn poisoned_connection() -> Arc<Mutex<SqliteConnection>> {
        let shared_connection = Arc::new(Mutex::new(establish_connection()));
        let connection_to_poison = shared_connection.clone();
        let result = catch_unwind(AssertUnwindSafe(move || {
            let _guard = connection_to_poison
                .lock()
                .expect("connection lock should be available before poisoning");
            resume_unwind(Box::new("poison advisory connection lock"));
        }));
        assert!(result.is_err());
        shared_connection
    }

    #[test]
    fn advisory_thresholds_returns_none_for_missing_account() {
        let (db, shared_connection) = create_factory();

        let thresholds = db
            .advisory_read()
            .advisory_thresholds_for_account(Uuid::new_v4())
            .expect("missing threshold read should succeed");

        assert_eq!(thresholds, None);
        drop(shared_connection);
    }

    #[test]
    fn advisory_thresholds_roundtrip() {
        let (db, shared_connection) = create_factory();
        let account = db
            .account_write()
            .create(
                "Advisory Test",
                "for threshold roundtrip",
                model::Environment::Paper,
                dec!(0),
                dec!(0),
            )
            .expect("account create");

        let account_id = account.id;
        db.advisory_write()
            .upsert_advisory_thresholds(account_id, dec!(30), dec!(40), dec!(15))
            .expect("threshold upsert");

        let thresholds = db
            .advisory_read()
            .advisory_thresholds_for_account(account_id)
            .expect("threshold read");
        assert_eq!(thresholds, Some((dec!(30), dec!(40), dec!(15))));
        drop(shared_connection);
    }

    #[test]
    fn advisory_thresholds_upsert_updates_existing_account_row() {
        let (db, shared_connection) = create_factory();
        let account = db
            .account_write()
            .create(
                "Advisory Update Test",
                "for threshold upsert updates",
                model::Environment::Paper,
                dec!(0),
                dec!(0),
            )
            .expect("account create");

        let account_id = account.id;
        db.advisory_write()
            .upsert_advisory_thresholds(account_id, dec!(30), dec!(40), dec!(15))
            .expect("initial threshold upsert");
        db.advisory_write()
            .upsert_advisory_thresholds(account_id, dec!(35), dec!(45), dec!(20))
            .expect("second threshold upsert should update existing row");

        let thresholds = db
            .advisory_read()
            .advisory_thresholds_for_account(account_id)
            .expect("threshold read");
        assert_eq!(thresholds, Some((dec!(35), dec!(45), dec!(20))));
        assert_eq!(advisory_row_count(&shared_connection, account_id), 1);
        drop(shared_connection);
    }

    #[test]
    fn advisory_threshold_row_debug_includes_decimal_fields() {
        let row = AdvisoryThresholdRow {
            sector_limit_pct: "30".to_string(),
            asset_class_limit_pct: "40".to_string(),
            single_position_limit_pct: "15".to_string(),
        };

        assert_eq!(
            format!("{row:?}"),
            "AdvisoryThresholdRow { sector_limit_pct: \"30\", asset_class_limit_pct: \"40\", single_position_limit_pct: \"15\" }"
        );
    }

    #[test]
    fn advisory_thresholds_read_reports_database_errors() {
        let (_db, shared_connection) = create_factory();
        diesel::sql_query("DROP TABLE advisory_thresholds")
            .execute(
                &mut *shared_connection
                    .lock()
                    .expect("connection lock should be available"),
            )
            .expect("advisory_thresholds table should drop");
        let mut advisory = AdvisoryDB {
            connection: shared_connection,
        };

        let error = advisory
            .advisory_thresholds_for_account(Uuid::new_v4())
            .expect_err("missing table should fail advisory threshold read");

        assert!(error.to_string().contains("advisory_thresholds"));
    }

    #[test]
    fn advisory_thresholds_upsert_reports_database_errors() {
        let (_db, shared_connection) = create_factory();
        diesel::sql_query("DROP TABLE advisory_thresholds")
            .execute(
                &mut *shared_connection
                    .lock()
                    .expect("connection lock should be available"),
            )
            .expect("advisory_thresholds table should drop");
        let mut advisory = AdvisoryDB {
            connection: shared_connection,
        };

        let error = advisory
            .upsert_advisory_thresholds(Uuid::new_v4(), dec!(30), dec!(40), dec!(15))
            .expect_err("missing table should fail advisory threshold upsert");

        assert!(error.to_string().contains("advisory_thresholds"));
    }

    #[test]
    fn advisory_thresholds_report_poisoned_connection_lock_errors() {
        let mut read_worker = AdvisoryDB {
            connection: poisoned_connection(),
        };
        let read_error = read_worker
            .advisory_thresholds_for_account(Uuid::new_v4())
            .expect_err("poisoned connection should fail advisory read");
        assert!(read_error
            .to_string()
            .contains("Failed to acquire connection lock"));

        let mut write_worker = AdvisoryDB {
            connection: poisoned_connection(),
        };
        let write_error = write_worker
            .upsert_advisory_thresholds(Uuid::new_v4(), dec!(30), dec!(40), dec!(15))
            .expect_err("poisoned connection should fail advisory upsert");
        assert!(write_error
            .to_string()
            .contains("Failed to acquire connection lock"));
    }

    #[test]
    fn advisory_thresholds_rejects_invalid_db_rows_for_each_decimal_field() {
        for (sector, asset_class, single_position, field) in [
            ("bad-number", "20", "15", "sector_limit_pct"),
            ("30", "bad-number", "15", "asset_class_limit_pct"),
            ("30", "20", "bad-number", "single_position_limit_pct"),
        ] {
            let (_db, shared_connection) = create_factory();
            let db = crate::SqliteDatabase::new_from(shared_connection.clone());
            let account = db
                .account_write()
                .create(
                    "Advisory Test",
                    "for invalid advisory row",
                    model::Environment::Paper,
                    dec!(0),
                    dec!(0),
                )
                .expect("account create");

            let mut advisory = AdvisoryDB {
                connection: shared_connection.clone(),
            };
            let account_id = account.id;
            insert_threshold_row(
                &shared_connection,
                account_id,
                sector,
                asset_class,
                single_position,
            );

            let err = advisory
                .advisory_thresholds_for_account(account_id)
                .expect_err("expected parse failure");
            assert!(err
                .to_string()
                .contains("Invalid advisory threshold decimal value in database"));
            assert!(
                err.to_string().contains(field),
                "expected field {field} in error: {err}"
            );
            drop(shared_connection);
        }
    }
}
