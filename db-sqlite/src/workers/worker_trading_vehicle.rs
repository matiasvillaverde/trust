use crate::error::{ConversionError, IntoDomainModel, IntoDomainModels};
use crate::schema::trading_vehicles;
use chrono::{NaiveDate, NaiveDateTime, Utc};
use diesel::prelude::*;
use diesel::OptionalExtension;
use model::database::TradingVehicleUpsert;
use model::{FixedIncomeTerms, TradingVehicle, TradingVehicleCategory};
use rust_decimal::Decimal;
use std::error::Error;
use std::str::FromStr;
use tracing::error;
use uuid::Uuid;

/// Worker for handling trading vehicle database operations
#[derive(Debug)]
pub struct WorkerTradingVehicle;
impl WorkerTradingVehicle {
    pub fn create(
        connection: &mut SqliteConnection,
        symbol: &str,
        isin: Option<&str>,
        category: &TradingVehicleCategory,
        broker: &str,
    ) -> Result<TradingVehicle, Box<dyn Error>> {
        let id = Uuid::new_v4().to_string();
        let now = Utc::now().naive_utc();

        let symbol_norm = symbol.trim().to_uppercase();
        let broker_norm = broker.trim().to_lowercase();
        let isin_norm = isin
            .map(|value| value.trim().to_uppercase())
            // Some brokers do not provide ISIN. Keep the DB constraint happy with a stable,
            // broker-scoped synthetic identifier.
            .or_else(|| Some(format!("{}:{}", broker_norm.to_uppercase(), symbol_norm)));

        let new_trading_vehicle = NewTradingVehicle {
            id,
            created_at: now,
            updated_at: now,
            deleted_at: None,
            symbol: symbol_norm,
            isin: isin_norm,
            category: category.to_string(),
            broker: broker_norm,
            broker_asset_id: None,
            exchange: None,
            broker_asset_class: None,
            broker_asset_status: None,
            tradable: None,
            marginable: None,
            shortable: None,
            easy_to_borrow: None,
            fractionable: None,
            fixed_income_face_value: None,
            fixed_income_coupon_rate_pct: None,
            fixed_income_maturity_date: None,
            fixed_income_coupon_frequency_per_year: None,
        };

        let tv = diesel::insert_into(trading_vehicles::table)
            .values(&new_trading_vehicle)
            .get_result::<TradingVehicleSQLite>(connection)
            .map_err(|error| {
                error!("Error creating price: {:?}", error);
                error
            })?
            .into_domain_model()?;
        Ok(tv)
    }

    pub fn upsert(
        connection: &mut SqliteConnection,
        input: TradingVehicleUpsert,
    ) -> Result<TradingVehicle, Box<dyn Error>> {
        let now = Utc::now().naive_utc();
        let new_trading_vehicle = build_upsert_row(connection, input, now)?;

        let tv = diesel::insert_into(trading_vehicles::table)
            .values(&new_trading_vehicle)
            .on_conflict((trading_vehicles::broker, trading_vehicles::symbol))
            .do_update()
            .set((
                trading_vehicles::updated_at.eq(now),
                trading_vehicles::deleted_at.eq::<Option<NaiveDateTime>>(None),
                trading_vehicles::isin.eq(new_trading_vehicle.isin.clone()),
                trading_vehicles::category.eq(new_trading_vehicle.category.clone()),
                trading_vehicles::broker_asset_id.eq(new_trading_vehicle.broker_asset_id.clone()),
                trading_vehicles::exchange.eq(new_trading_vehicle.exchange.clone()),
                trading_vehicles::broker_asset_class
                    .eq(new_trading_vehicle.broker_asset_class.clone()),
                trading_vehicles::broker_asset_status
                    .eq(new_trading_vehicle.broker_asset_status.clone()),
                trading_vehicles::tradable.eq(new_trading_vehicle.tradable),
                trading_vehicles::marginable.eq(new_trading_vehicle.marginable),
                trading_vehicles::shortable.eq(new_trading_vehicle.shortable),
                trading_vehicles::easy_to_borrow.eq(new_trading_vehicle.easy_to_borrow),
                trading_vehicles::fractionable.eq(new_trading_vehicle.fractionable),
                trading_vehicles::fixed_income_face_value
                    .eq(new_trading_vehicle.fixed_income_face_value.clone()),
                trading_vehicles::fixed_income_coupon_rate_pct
                    .eq(new_trading_vehicle.fixed_income_coupon_rate_pct.clone()),
                trading_vehicles::fixed_income_maturity_date
                    .eq(new_trading_vehicle.fixed_income_maturity_date),
                trading_vehicles::fixed_income_coupon_frequency_per_year
                    .eq(new_trading_vehicle.fixed_income_coupon_frequency_per_year),
            ))
            .get_result::<TradingVehicleSQLite>(connection)
            .map_err(|error| {
                error!("Error upserting trading vehicle: {:?}", error);
                error
            })?
            .into_domain_model()?;

        Ok(tv)
    }

    pub fn read_all(
        connection: &mut SqliteConnection,
    ) -> Result<Vec<TradingVehicle>, Box<dyn Error>> {
        let tvs = trading_vehicles::table
            .filter(trading_vehicles::deleted_at.is_null())
            .load::<TradingVehicleSQLite>(connection)
            .map_err(|error| {
                error!("Error creating price: {:?}", error);
                error
            })?
            .into_domain_models()?;
        Ok(tvs)
    }

    pub fn read(
        connection: &mut SqliteConnection,
        id: Uuid,
    ) -> Result<TradingVehicle, Box<dyn Error>> {
        let tv = trading_vehicles::table
            .filter(trading_vehicles::id.eq(id.to_string()))
            .filter(trading_vehicles::deleted_at.is_null())
            .first::<TradingVehicleSQLite>(connection)
            .map_err(|error| {
                error!("Error reading trading vehicle: {:?}", error);
                error
            })?
            .into_domain_model()?;
        Ok(tv)
    }
}

fn build_upsert_row(
    connection: &mut SqliteConnection,
    input: TradingVehicleUpsert,
    now: NaiveDateTime,
) -> Result<NewTradingVehicle, Box<dyn Error>> {
    let symbol_norm = input.symbol.trim().to_uppercase();
    let broker_norm = input.broker.trim().to_lowercase();
    let broker_norm_upper = input.broker.trim().to_uppercase();
    let provided_isin = input
        .isin
        .as_deref()
        .map(|value| value.trim().to_uppercase());
    let isin_norm = normalized_isin(connection, &broker_norm, &symbol_norm, provided_isin)?
        .or_else(|| Some(format!("{}:{}", broker_norm_upper, symbol_norm)));
    let fixed_income_face_value = fixed_income_face_value(input.fixed_income.as_ref());
    let fixed_income_coupon_rate_pct = fixed_income_coupon_rate_pct(input.fixed_income.as_ref());
    let fixed_income_maturity_date = input
        .fixed_income
        .as_ref()
        .and_then(|terms| terms.maturity_date);
    let fixed_income_coupon_frequency_per_year = input
        .fixed_income
        .as_ref()
        .and_then(|terms| terms.coupon_frequency_per_year)
        .map(i32::from);

    Ok(NewTradingVehicle {
        id: Uuid::new_v4().to_string(),
        created_at: now,
        updated_at: now,
        deleted_at: None,
        symbol: symbol_norm,
        isin: isin_norm,
        category: input.category.to_string(),
        broker: broker_norm,
        broker_asset_id: trim_optional(input.broker_asset_id.as_deref()),
        exchange: trim_optional(input.exchange.as_deref()),
        broker_asset_class: trim_optional(input.broker_asset_class.as_deref()),
        broker_asset_status: trim_optional(input.broker_asset_status.as_deref()),
        tradable: input.tradable,
        marginable: input.marginable,
        shortable: input.shortable,
        easy_to_borrow: input.easy_to_borrow,
        fractionable: input.fractionable,
        fixed_income_face_value,
        fixed_income_coupon_rate_pct,
        fixed_income_maturity_date,
        fixed_income_coupon_frequency_per_year,
    })
}

fn normalized_isin(
    connection: &mut SqliteConnection,
    broker_norm: &str,
    symbol_norm: &str,
    provided_isin: Option<String>,
) -> Result<Option<String>, diesel::result::Error> {
    let existing_isin = trading_vehicles::table
        .filter(trading_vehicles::broker.eq(broker_norm))
        .filter(trading_vehicles::symbol.eq(symbol_norm))
        .select(trading_vehicles::isin)
        .first::<Option<String>>(connection)
        .optional()?
        .flatten();
    Ok(provided_isin.or(existing_isin))
}

fn trim_optional(value: Option<&str>) -> Option<String> {
    value.map(|v| v.trim().to_string())
}

fn fixed_income_face_value(terms: Option<&FixedIncomeTerms>) -> Option<String> {
    terms.and_then(|terms| terms.face_value.as_ref().map(ToString::to_string))
}

fn fixed_income_coupon_rate_pct(terms: Option<&FixedIncomeTerms>) -> Option<String> {
    terms.and_then(|terms| {
        terms
            .annual_coupon_rate_pct
            .as_ref()
            .map(ToString::to_string)
    })
}

#[derive(Debug, Queryable, Identifiable, AsChangeset, Insertable)]
#[diesel(table_name = trading_vehicles)]
struct TradingVehicleSQLite {
    id: String,
    created_at: NaiveDateTime,
    updated_at: NaiveDateTime,
    deleted_at: Option<NaiveDateTime>,
    symbol: String,
    isin: Option<String>,
    category: String,
    broker: String,
    broker_asset_id: Option<String>,
    exchange: Option<String>,
    broker_asset_class: Option<String>,
    broker_asset_status: Option<String>,
    tradable: Option<bool>,
    marginable: Option<bool>,
    shortable: Option<bool>,
    easy_to_borrow: Option<bool>,
    fractionable: Option<bool>,
    fixed_income_face_value: Option<String>,
    fixed_income_coupon_rate_pct: Option<String>,
    fixed_income_maturity_date: Option<NaiveDate>,
    fixed_income_coupon_frequency_per_year: Option<i32>,
}

impl TryFrom<TradingVehicleSQLite> for TradingVehicle {
    type Error = ConversionError;

    fn try_from(value: TradingVehicleSQLite) -> Result<Self, Self::Error> {
        Ok(TradingVehicle {
            id: Uuid::parse_str(&value.id)
                .map_err(|_| ConversionError::new("id", "Failed to parse trading vehicle ID"))?,
            created_at: value.created_at,
            updated_at: value.updated_at,
            deleted_at: value.deleted_at,
            symbol: value.symbol,
            isin: value.isin,
            category: TradingVehicleCategory::from_str(&value.category).map_err(|_| {
                ConversionError::new("category", "Failed to parse trading vehicle category")
            })?,
            broker: value.broker,
            broker_asset_id: value.broker_asset_id,
            exchange: value.exchange,
            broker_asset_class: value.broker_asset_class,
            broker_asset_status: value.broker_asset_status,
            tradable: value.tradable,
            marginable: value.marginable,
            shortable: value.shortable,
            easy_to_borrow: value.easy_to_borrow,
            fractionable: value.fractionable,
            fixed_income: fixed_income_terms(
                value.fixed_income_face_value,
                value.fixed_income_coupon_rate_pct,
                value.fixed_income_maturity_date,
                value.fixed_income_coupon_frequency_per_year,
            )?,
        })
    }
}

fn fixed_income_terms(
    face_value: Option<String>,
    annual_coupon_rate_pct: Option<String>,
    maturity_date: Option<NaiveDate>,
    coupon_frequency_per_year: Option<i32>,
) -> Result<Option<FixedIncomeTerms>, ConversionError> {
    let coupon_frequency_per_year = coupon_frequency_per_year
        .map(u16::try_from)
        .transpose()
        .map_err(|_| {
            ConversionError::new(
                "fixed_income_coupon_frequency_per_year",
                "Invalid coupon frequency",
            )
        })?;

    let terms = FixedIncomeTerms {
        face_value: parse_optional_decimal(face_value, "fixed_income_face_value")?,
        annual_coupon_rate_pct: parse_optional_decimal(
            annual_coupon_rate_pct,
            "fixed_income_coupon_rate_pct",
        )?,
        maturity_date,
        coupon_frequency_per_year,
    };

    if terms.is_empty() {
        Ok(None)
    } else {
        Ok(Some(terms))
    }
}

fn parse_optional_decimal(
    value: Option<String>,
    field: &'static str,
) -> Result<Option<Decimal>, ConversionError> {
    value
        .map(|raw| {
            Decimal::from_str(&raw)
                .map_err(|_| ConversionError::new(field, "Failed to parse decimal"))
        })
        .transpose()
}

impl IntoDomainModel<TradingVehicle> for TradingVehicleSQLite {
    fn into_domain_model(self) -> Result<TradingVehicle, Box<dyn Error>> {
        self.try_into().map_err(Into::into)
    }
}

#[derive(Debug, Insertable)]
#[diesel(table_name = trading_vehicles)]
pub struct NewTradingVehicle {
    id: String,
    created_at: NaiveDateTime,
    updated_at: NaiveDateTime,
    deleted_at: Option<NaiveDateTime>,
    symbol: String,
    isin: Option<String>,
    category: String,
    broker: String,
    broker_asset_id: Option<String>,
    exchange: Option<String>,
    broker_asset_class: Option<String>,
    broker_asset_status: Option<String>,
    tradable: Option<bool>,
    marginable: Option<bool>,
    shortable: Option<bool>,
    easy_to_borrow: Option<bool>,
    fractionable: Option<bool>,
    fixed_income_face_value: Option<String>,
    fixed_income_coupon_rate_pct: Option<String>,
    fixed_income_maturity_date: Option<NaiveDate>,
    fixed_income_coupon_frequency_per_year: Option<i32>,
}
#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;
    use diesel_migrations::*;
    use rust_decimal_macros::dec;

    pub const MIGRATIONS: EmbeddedMigrations = embed_migrations!();

    fn establish_connection() -> SqliteConnection {
        let mut connection = SqliteConnection::establish(":memory:").unwrap();
        // This will run the necessary migrations.
        connection.run_pending_migrations(MIGRATIONS).unwrap();
        connection.begin_test_transaction().unwrap();
        connection
    }

    fn create_apple_trading_vehicle(conn: &mut SqliteConnection) -> TradingVehicle {
        WorkerTradingVehicle::create(
            conn,
            "AAPl",
            Some("uS0378331005"),
            &TradingVehicleCategory::Fiat,
            "NASDAQ",
        )
        .expect("Error creating trading_vehicle")
    }

    fn base_sqlite_trading_vehicle() -> TradingVehicleSQLite {
        let now = Utc::now().naive_utc();
        TradingVehicleSQLite {
            id: Uuid::new_v4().to_string(),
            created_at: now,
            updated_at: now,
            deleted_at: None,
            symbol: "AAPL".to_string(),
            isin: Some(Uuid::new_v4().to_string()),
            category: TradingVehicleCategory::Stock.to_string(),
            broker: "alpaca".to_string(),
            broker_asset_id: None,
            exchange: None,
            broker_asset_class: None,
            broker_asset_status: None,
            tradable: None,
            marginable: None,
            shortable: None,
            easy_to_borrow: None,
            fractionable: None,
            fixed_income_face_value: None,
            fixed_income_coupon_rate_pct: None,
            fixed_income_maturity_date: None,
            fixed_income_coupon_frequency_per_year: None,
        }
    }

    fn stock_upsert(symbol: &str) -> TradingVehicleUpsert {
        TradingVehicleUpsert {
            symbol: symbol.to_string(),
            isin: None,
            category: TradingVehicleCategory::Stock,
            broker: "alpaca".to_string(),
            broker_asset_id: None,
            exchange: None,
            broker_asset_class: None,
            broker_asset_status: None,
            tradable: None,
            marginable: None,
            shortable: None,
            easy_to_borrow: None,
            fractionable: None,
            fixed_income: None,
        }
    }

    fn assert_conversion_error(row: TradingVehicleSQLite, field: &str) {
        let error = TradingVehicle::try_from(row).expect_err("corrupt trading vehicle must fail");
        assert!(
            error.to_string().contains(field),
            "expected conversion error to mention {field:?}, got {error:?}"
        );
    }

    fn assert_error_mentions(error: Box<dyn Error>, expected: &str) {
        let message = error.to_string();
        assert!(
            message.contains(expected),
            "expected error to mention {expected:?}, got {message:?}"
        );
    }

    #[test]
    fn test_create_trading_vehicle() {
        let mut conn = establish_connection();

        let trading_vehicle = create_apple_trading_vehicle(&mut conn);

        assert_eq!(trading_vehicle.symbol, "AAPL"); // symbol should be uppercase
        assert_eq!(trading_vehicle.isin, Some("US0378331005".to_string())); // isin should be uppercase
        assert_eq!(trading_vehicle.category, TradingVehicleCategory::Fiat);
        assert_eq!(trading_vehicle.broker, "nasdaq"); // broker should be lowercase
        assert_eq!(trading_vehicle.updated_at, trading_vehicle.created_at); // created_at and updated_at should be the same
        assert_eq!(trading_vehicle.created_at, trading_vehicle.updated_at); // created_at and updated_at should be the same
        assert_eq!(trading_vehicle.deleted_at, None);
    }

    #[test]
    fn test_create_trading_vehicle_same_broker_symbol_conflicts() {
        let mut conn = establish_connection();
        create_apple_trading_vehicle(&mut conn);
        WorkerTradingVehicle::create(
            &mut conn,
            "AAPl",
            Some("uS0378331005"),
            &TradingVehicleCategory::Fiat,
            "NASDAQ",
        )
        .expect_err("Error creating trading_vehicle with same broker+symbol");
    }

    #[test]
    fn test_read_trading_vehicle() {
        let mut conn = establish_connection();

        WorkerTradingVehicle::create(
            &mut conn,
            "TSLA",
            Some("US88160R1014"),
            &TradingVehicleCategory::Fiat,
            "NASDAQ",
        )
        .unwrap();

        create_apple_trading_vehicle(&mut conn);

        let read_trading_vehicles =
            WorkerTradingVehicle::read_all(&mut conn).expect("Error reading trading_vehicle");

        assert_eq!(read_trading_vehicles.len(), 2);
    }

    #[test]
    fn read_returns_single_vehicle_and_hides_soft_deleted_rows() {
        let mut conn = establish_connection();
        let created = create_apple_trading_vehicle(&mut conn);

        let read = WorkerTradingVehicle::read(&mut conn, created.id)
            .expect("trading vehicle should read by id");
        assert_eq!(read.id, created.id);

        diesel::update(
            trading_vehicles::table.filter(trading_vehicles::id.eq(created.id.to_string())),
        )
        .set(trading_vehicles::deleted_at.eq(Some(Utc::now().naive_utc())))
        .execute(&mut conn)
        .expect("trading vehicle should be soft deleted");

        WorkerTradingVehicle::read(&mut conn, created.id)
            .expect_err("soft-deleted trading vehicle should not read by id");
        let all = WorkerTradingVehicle::read_all(&mut conn).expect("read all should succeed");
        assert!(all.is_empty());
    }

    #[test]
    fn test_upsert_updates_metadata_fields() {
        let mut conn = establish_connection();
        let input = TradingVehicleUpsert {
            symbol: "aapl".to_string(),
            isin: None,
            category: TradingVehicleCategory::Stock,
            broker: "alpaca".to_string(),
            broker_asset_id: Some("904837e3-3b76-47ec-b432-046db621571b".to_string()),
            exchange: Some("NASDAQ".to_string()),
            broker_asset_class: Some("us_equity".to_string()),
            broker_asset_status: Some("active".to_string()),
            tradable: Some(true),
            marginable: Some(true),
            shortable: Some(false),
            easy_to_borrow: Some(false),
            fractionable: Some(true),
            fixed_income: None,
        };

        let created = WorkerTradingVehicle::upsert(&mut conn, input.clone()).unwrap();
        assert_eq!(created.symbol, "AAPL");
        assert_eq!(created.broker, "alpaca");
        assert_eq!(created.broker_asset_id, input.broker_asset_id);
        assert_eq!(created.exchange, input.exchange);

        let mut updated_input = input;
        updated_input.exchange = Some("NYSE".to_string());
        updated_input.shortable = Some(true);
        let updated = WorkerTradingVehicle::upsert(&mut conn, updated_input.clone()).unwrap();
        assert_eq!(updated.exchange, updated_input.exchange);
        assert_eq!(updated.shortable, updated_input.shortable);
    }

    #[test]
    fn test_create_multi_asset_categories() {
        let mut conn = establish_connection();

        let etf = WorkerTradingVehicle::create(
            &mut conn,
            "SPY",
            None,
            &TradingVehicleCategory::Etf,
            "ibkr",
        )
        .unwrap();
        let bond = WorkerTradingVehicle::create(
            &mut conn,
            "9128285M8",
            None,
            &TradingVehicleCategory::Bond,
            "ibkr",
        )
        .unwrap();

        assert_eq!(etf.category, TradingVehicleCategory::Etf);
        assert_eq!(bond.category, TradingVehicleCategory::Bond);
    }

    #[test]
    fn test_upsert_persists_fixed_income_terms() {
        let mut conn = establish_connection();
        let maturity_date = NaiveDate::from_ymd_opt(2034, 5, 15).unwrap();

        let created = WorkerTradingVehicle::upsert(
            &mut conn,
            TradingVehicleUpsert {
                symbol: "9128285M8".to_string(),
                isin: None,
                category: TradingVehicleCategory::Bond,
                broker: "ibkr".to_string(),
                broker_asset_id: Some("123456".to_string()),
                exchange: Some("SMART".to_string()),
                broker_asset_class: Some("bond".to_string()),
                broker_asset_status: None,
                tradable: None,
                marginable: None,
                shortable: None,
                easy_to_borrow: None,
                fractionable: None,
                fixed_income: Some(FixedIncomeTerms {
                    face_value: Some(dec!(1000)),
                    annual_coupon_rate_pct: Some(dec!(4.625)),
                    maturity_date: Some(maturity_date),
                    coupon_frequency_per_year: Some(2),
                }),
            },
        )
        .unwrap();

        let terms = created.fixed_income.unwrap();
        assert_eq!(terms.face_value, Some(dec!(1000)));
        assert_eq!(terms.annual_coupon_rate_pct, Some(dec!(4.625)));
        assert_eq!(terms.maturity_date, Some(maturity_date));
        assert_eq!(terms.coupon_frequency_per_year, Some(2));
    }

    #[test]
    fn test_upsert_preserves_existing_real_isin_when_input_is_none() {
        let mut conn = establish_connection();

        let created = WorkerTradingVehicle::create(
            &mut conn,
            "AAPL",
            Some("US0378331005"),
            &TradingVehicleCategory::Stock,
            "alpaca",
        )
        .unwrap();
        assert_eq!(created.isin, Some("US0378331005".to_string()));

        let updated = WorkerTradingVehicle::upsert(
            &mut conn,
            TradingVehicleUpsert {
                symbol: "aapl".to_string(),
                isin: None,
                category: TradingVehicleCategory::Stock,
                broker: "alpaca".to_string(),
                broker_asset_id: Some("904837e3-3b76-47ec-b432-046db621571b".to_string()),
                exchange: Some("NASDAQ".to_string()),
                broker_asset_class: Some("us_equity".to_string()),
                broker_asset_status: Some("active".to_string()),
                tradable: Some(true),
                marginable: Some(true),
                shortable: Some(false),
                easy_to_borrow: Some(false),
                fractionable: Some(true),
                fixed_income: None,
            },
        )
        .unwrap();

        assert_eq!(updated.isin, Some("US0378331005".to_string()));
        assert_eq!(
            updated.broker_asset_id,
            Some("904837e3-3b76-47ec-b432-046db621571b".to_string())
        );
    }

    #[test]
    fn trading_vehicle_conversion_reports_corrupt_fields() {
        assert_conversion_error(
            TradingVehicleSQLite {
                id: "not-a-uuid".to_string(),
                ..base_sqlite_trading_vehicle()
            },
            "id",
        );
        assert_conversion_error(
            TradingVehicleSQLite {
                category: "warrant".to_string(),
                ..base_sqlite_trading_vehicle()
            },
            "category",
        );
        assert_conversion_error(
            TradingVehicleSQLite {
                fixed_income_face_value: Some("not-decimal".to_string()),
                ..base_sqlite_trading_vehicle()
            },
            "fixed_income_face_value",
        );
        assert_conversion_error(
            TradingVehicleSQLite {
                fixed_income_coupon_rate_pct: Some("not-decimal".to_string()),
                ..base_sqlite_trading_vehicle()
            },
            "fixed_income_coupon_rate_pct",
        );
        assert_conversion_error(
            TradingVehicleSQLite {
                fixed_income_coupon_frequency_per_year: Some(-1),
                ..base_sqlite_trading_vehicle()
            },
            "fixed_income_coupon_frequency_per_year",
        );
    }

    #[test]
    fn trading_vehicle_conversion_keeps_partial_fixed_income_terms() {
        let maturity_date = NaiveDate::from_ymd_opt(2034, 5, 15).unwrap();
        let vehicle = TradingVehicle::try_from(TradingVehicleSQLite {
            fixed_income_maturity_date: Some(maturity_date),
            fixed_income_coupon_frequency_per_year: Some(4),
            ..base_sqlite_trading_vehicle()
        })
        .expect("partial fixed-income terms should convert");

        let terms = vehicle
            .fixed_income
            .expect("partial fixed-income terms should be retained");
        assert_eq!(terms.face_value, None);
        assert_eq!(terms.annual_coupon_rate_pct, None);
        assert_eq!(terms.maturity_date, Some(maturity_date));
        assert_eq!(terms.coupon_frequency_per_year, Some(4));
    }

    #[test]
    fn read_all_surfaces_corrupt_row_id() {
        let mut conn = establish_connection();

        diesel::insert_into(trading_vehicles::table)
            .values(TradingVehicleSQLite {
                id: "not-a-uuid".to_string(),
                ..base_sqlite_trading_vehicle()
            })
            .execute(&mut conn)
            .expect("corrupt trading vehicle row should insert for conversion test");

        let error = WorkerTradingVehicle::read_all(&mut conn)
            .expect_err("corrupt row should fail conversion during read");

        assert_error_mentions(error, "id");
    }

    #[test]
    fn trading_vehicle_upsert_reports_insert_errors_after_lookup_succeeds() {
        let mut conn = establish_connection();
        diesel::sql_query(
            "CREATE TRIGGER fail_trading_vehicle_insert \
             BEFORE INSERT ON trading_vehicles \
             BEGIN \
             SELECT RAISE(ABORT, 'forced trading vehicle insert failure'); \
             END",
        )
        .execute(&mut conn)
        .expect("insert failure trigger should be created");

        let error = WorkerTradingVehicle::upsert(&mut conn, stock_upsert("AAPL"))
            .expect_err("trigger should fail trading vehicle upsert");

        assert_error_mentions(error, "forced trading vehicle insert failure");
    }

    #[test]
    fn trading_vehicle_worker_reports_missing_table_errors() {
        let mut conn = establish_connection();
        diesel::sql_query("DROP TABLE trading_vehicles")
            .execute(&mut conn)
            .expect("trading_vehicles table should drop");
        let id = Uuid::new_v4();

        let error = WorkerTradingVehicle::create(
            &mut conn,
            "AAPL",
            Some("US0378331005"),
            &TradingVehicleCategory::Stock,
            "alpaca",
        )
        .expect_err("missing table should fail trading vehicle create");
        assert_error_mentions(error, "trading_vehicles");

        let error = WorkerTradingVehicle::upsert(&mut conn, stock_upsert("AAPL"))
            .expect_err("missing table should fail trading vehicle upsert");
        assert_error_mentions(error, "trading_vehicles");

        let error = WorkerTradingVehicle::read_all(&mut conn)
            .expect_err("missing table should fail trading vehicle read all");
        assert_error_mentions(error, "trading_vehicles");

        let error = WorkerTradingVehicle::read(&mut conn, id)
            .expect_err("missing table should fail trading vehicle read");
        assert_error_mentions(error, "trading_vehicles");
    }
}
