use crate::error::ConversionError;
use chrono::Utc;
use diesel::prelude::*;
use diesel::sql_query;
use diesel::sql_types::Text;
use model::{AdvisoryRead, AdvisoryWrite};
use rust_decimal::Decimal;
use std::error::Error;
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
    ) -> Result<Option<(Decimal, Decimal, Decimal)>, Box<dyn Error>> {
        let rows: Vec<AdvisoryThresholdRow> = sql_query(
            "SELECT sector_limit_pct, asset_class_limit_pct, single_position_limit_pct \
            FROM advisory_thresholds WHERE account_id = ?1",
        )
        .bind::<Text, _>(account_id.to_string())
        .load(&mut *self.connection.lock().unwrap_or_else(|e| {
            eprintln!("Failed to acquire connection lock: {e}");
            std::process::exit(1);
        }))
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
        .execute(
            &mut *self.connection.lock().unwrap_or_else(|e| {
                eprintln!("Failed to acquire connection lock: {e}");
                std::process::exit(1);
            }),
        )
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
    use diesel_migrations::*;
    use model::DatabaseFactory;
    use rust_decimal_macros::dec;
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
    fn advisory_thresholds_rejects_invalid_db_rows() {
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
        .bind::<Text, _>("bad-number")
        .bind::<Text, _>(dec!(20).to_string())
        .bind::<Text, _>(dec!(15).to_string())
        .execute(
            &mut *shared_connection
                .lock()
                .unwrap_or_else(|e| {
                    panic!("Failed to acquire connection lock: {e}");
                }),
        )
        .expect("insert invalid advisory row");

        let err = advisory
            .advisory_thresholds_for_account(account_id)
            .expect_err("expected parse failure");
        assert!(err
            .to_string()
            .contains("Invalid advisory threshold decimal value in database"));
        drop(shared_connection);
    }
}
