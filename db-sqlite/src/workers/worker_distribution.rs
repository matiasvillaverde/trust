use crate::error::{ConversionError, IntoDomainModel};
use crate::schema::{distribution_history, distribution_rules};
use chrono::{NaiveDateTime, Utc};
use diesel::prelude::*;
use model::{
    DistributionExecutionPlan, DistributionHistory, DistributionRead, DistributionRules,
    DistributionRulesNotFound, DistributionWrite,
};
use rust_decimal::Decimal;
use std::error::Error;
use std::str::FromStr;
use std::sync::{Arc, Mutex, MutexGuard};
use tracing::error;
use uuid::Uuid;

use super::WorkerTransaction;

/// Database worker for distribution operations
pub struct DistributionDB {
    pub connection: Arc<Mutex<SqliteConnection>>,
}

impl std::fmt::Debug for DistributionDB {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DistributionDB")
            .field("connection", &"Arc<Mutex<SqliteConnection>>")
            .finish()
    }
}

impl DistributionDB {
    fn connection_guard(&self) -> Result<MutexGuard<'_, SqliteConnection>, Box<dyn Error>> {
        self.connection.lock().map_err(|error| {
            format!("failed to acquire distribution database connection lock: {error}").into()
        })
    }
}

impl DistributionRead for DistributionDB {
    fn for_account(&mut self, account_id: Uuid) -> Result<DistributionRules, Box<dyn Error>> {
        let mut guard = self.connection_guard()?;
        let connection: &mut SqliteConnection = &mut guard;

        let rules = distribution_rules::table
            .filter(distribution_rules::account_id.eq(account_id.to_string()))
            .first::<DistributionRulesSQLite>(connection)
            .optional()
            .map_err(|error| {
                error!("Error reading distribution rules: {:?}", error);
                error
            })?;

        match rules {
            Some(rule) => rule.into_domain_model(),
            None => Err(DistributionRulesNotFound { account_id }.into()),
        }
    }

    fn history_for_account(
        &mut self,
        account_id: Uuid,
    ) -> Result<Vec<DistributionHistory>, Box<dyn Error>> {
        let mut guard = self.connection_guard()?;
        let connection: &mut SqliteConnection = &mut guard;

        distribution_history::table
            .filter(distribution_history::source_account_id.eq(account_id.to_string()))
            .order(distribution_history::distribution_date.desc())
            .load::<DistributionHistorySQLite>(connection)
            .map_err(|error| {
                error!("Error reading distribution history: {:?}", error);
                error
            })?
            .into_iter()
            .map(IntoDomainModel::into_domain_model)
            .collect()
    }
}

impl DistributionWrite for DistributionDB {
    fn create_or_update(
        &mut self,
        account_id: Uuid,
        earnings_percent: Decimal,
        tax_percent: Decimal,
        reinvestment_percent: Decimal,
        minimum_threshold: Decimal,
        configuration_password_hash: &str,
    ) -> Result<DistributionRules, Box<dyn Error>> {
        self.create_or_update_with_insurance(
            account_id,
            earnings_percent,
            tax_percent,
            reinvestment_percent,
            Decimal::ZERO,
            minimum_threshold,
            configuration_password_hash,
        )
    }

    fn create_or_update_with_insurance(
        &mut self,
        account_id: Uuid,
        earnings_percent: Decimal,
        tax_percent: Decimal,
        reinvestment_percent: Decimal,
        insurance_percent: Decimal,
        minimum_threshold: Decimal,
        configuration_password_hash: &str,
    ) -> Result<DistributionRules, Box<dyn Error>> {
        let uuid = Uuid::new_v4().to_string();
        let now = Utc::now().naive_utc();

        let new_rules = NewDistributionRules {
            id: uuid,
            created_at: now,
            updated_at: now,
            account_id: account_id.to_string(),
            earnings_percent: earnings_percent.to_string(),
            tax_percent: tax_percent.to_string(),
            reinvestment_percent: reinvestment_percent.to_string(),
            insurance_percent: insurance_percent.to_string(),
            minimum_threshold: minimum_threshold.to_string(),
            configuration_password_hash: configuration_password_hash.to_string(),
        };

        let mut guard = self.connection_guard()?;
        let connection: &mut SqliteConnection = &mut guard;

        let existing = distribution_rules::table
            .filter(distribution_rules::account_id.eq(account_id.to_string()))
            .first::<DistributionRulesSQLite>(connection)
            .optional()
            .map_err(|error| {
                error!("Error reading existing distribution rules: {:?}", error);
                error
            })?;

        if existing.is_some() {
            diesel::update(
                distribution_rules::table
                    .filter(distribution_rules::account_id.eq(account_id.to_string())),
            )
            .set((
                distribution_rules::earnings_percent.eq(&new_rules.earnings_percent),
                distribution_rules::tax_percent.eq(&new_rules.tax_percent),
                distribution_rules::reinvestment_percent.eq(&new_rules.reinvestment_percent),
                distribution_rules::insurance_percent.eq(&new_rules.insurance_percent),
                distribution_rules::minimum_threshold.eq(&new_rules.minimum_threshold),
                distribution_rules::configuration_password_hash
                    .eq(&new_rules.configuration_password_hash),
                distribution_rules::updated_at.eq(&new_rules.updated_at),
            ))
            .get_result::<DistributionRulesSQLite>(connection)
            .map_err(|error| {
                error!("Error updating distribution rules: {:?}", error);
                error
            })?
            .into_domain_model()
        } else {
            diesel::insert_into(distribution_rules::table)
                .values(&new_rules)
                .get_result::<DistributionRulesSQLite>(connection)
                .map_err(|error| {
                    error!("Error creating distribution rules: {:?}", error);
                    error
                })?
                .into_domain_model()
        }
    }

    fn create_history(
        &mut self,
        source_account_id: Uuid,
        trade_id: Option<Uuid>,
        original_amount: Decimal,
        distribution_date: NaiveDateTime,
        earnings_amount: Option<Decimal>,
        tax_amount: Option<Decimal>,
        reinvestment_amount: Option<Decimal>,
    ) -> Result<DistributionHistory, Box<dyn Error>> {
        self.create_history_with_insurance(
            source_account_id,
            trade_id,
            original_amount,
            distribution_date,
            earnings_amount,
            tax_amount,
            reinvestment_amount,
            None,
        )
    }

    fn create_history_with_insurance(
        &mut self,
        source_account_id: Uuid,
        trade_id: Option<Uuid>,
        original_amount: Decimal,
        distribution_date: NaiveDateTime,
        earnings_amount: Option<Decimal>,
        tax_amount: Option<Decimal>,
        reinvestment_amount: Option<Decimal>,
        insurance_amount: Option<Decimal>,
    ) -> Result<DistributionHistory, Box<dyn Error>> {
        let mut guard = self.connection_guard()?;
        let connection: &mut SqliteConnection = &mut guard;

        let now = Utc::now().naive_utc();
        let new_history = NewDistributionHistory {
            id: Uuid::new_v4().to_string(),
            created_at: now,
            updated_at: now,
            source_account_id: source_account_id.to_string(),
            trade_id: trade_id.map(|id| id.to_string()),
            original_amount: original_amount.to_string(),
            distribution_date,
            earnings_amount: earnings_amount.map(|amount| amount.to_string()),
            tax_amount: tax_amount.map(|amount| amount.to_string()),
            reinvestment_amount: reinvestment_amount.map(|amount| amount.to_string()),
            insurance_amount: insurance_amount.map(|amount| amount.to_string()),
        };

        diesel::insert_into(distribution_history::table)
            .values(&new_history)
            .get_result::<DistributionHistorySQLite>(connection)
            .map_err(|error| {
                error!("Error creating distribution history: {:?}", error);
                error
            })?
            .into_domain_model()
    }

    fn execute_distribution_plan_atomic(
        &mut self,
        plan: &DistributionExecutionPlan,
    ) -> Result<Vec<Uuid>, Box<dyn Error>> {
        if plan.legs.is_empty() {
            return Err("Distribution plan must contain at least one transfer leg".into());
        }

        let mut guard = self.connection_guard()?;
        let connection: &mut SqliteConnection = &mut guard;

        connection.transaction::<Vec<Uuid>, Box<dyn Error>, _>(|conn| {
            let mut deposit_ids: Vec<Uuid> = Vec::new();

            for leg in &plan.legs {
                if leg.amount <= Decimal::ZERO {
                    return Err("Distribution leg amount must be positive".into());
                }

                let withdrawal_amount = Decimal::ZERO
                    .checked_sub(leg.amount)
                    .ok_or("Invalid withdrawal amount")?;

                let withdrawal_id = leg.forced_withdrawal_tx_id.unwrap_or_else(Uuid::new_v4);
                let deposit_id = leg.forced_deposit_tx_id.unwrap_or_else(Uuid::new_v4);

                WorkerTransaction::create_transaction_with_id(
                    conn,
                    withdrawal_id,
                    plan.source_account_id,
                    withdrawal_amount,
                    &plan.currency,
                    leg.withdrawal_category,
                )?;

                let deposit_tx = WorkerTransaction::create_transaction_with_id(
                    conn,
                    deposit_id,
                    leg.to_account_id,
                    leg.amount,
                    &plan.currency,
                    leg.deposit_category,
                )?;

                deposit_ids.push(deposit_tx.id);
            }

            let now = Utc::now().naive_utc();
            let new_history = NewDistributionHistory {
                id: Uuid::new_v4().to_string(),
                created_at: now,
                updated_at: now,
                source_account_id: plan.source_account_id.to_string(),
                trade_id: plan.trade_id.map(|id| id.to_string()),
                original_amount: plan.original_amount.to_string(),
                distribution_date: plan.distribution_date,
                earnings_amount: plan.earnings_amount.map(|amount| amount.to_string()),
                tax_amount: plan.tax_amount.map(|amount| amount.to_string()),
                reinvestment_amount: plan.reinvestment_amount.map(|amount| amount.to_string()),
                insurance_amount: plan.insurance_amount.map(|amount| amount.to_string()),
            };

            diesel::insert_into(distribution_history::table)
                .values(&new_history)
                .execute(conn)
                .map_err(|error| {
                    error!("Error creating distribution history: {:?}", error);
                    error
                })?;

            Ok(deposit_ids)
        })
    }
}

#[derive(Debug, Queryable, Identifiable, AsChangeset, Insertable)]
#[diesel(table_name = distribution_rules)]
#[diesel(primary_key(id))]
#[diesel(treat_none_as_null = true)]
pub struct DistributionRulesSQLite {
    pub id: String,
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
    pub account_id: String,
    pub earnings_percent: String,
    pub tax_percent: String,
    pub reinvestment_percent: String,
    pub minimum_threshold: String,
    pub configuration_password_hash: String,
    pub insurance_percent: String,
}

impl TryFrom<DistributionRulesSQLite> for DistributionRules {
    type Error = ConversionError;

    fn try_from(value: DistributionRulesSQLite) -> Result<Self, Self::Error> {
        Ok(DistributionRules {
            id: Uuid::parse_str(&value.id)
                .map_err(|_| ConversionError::new("id", "Failed to parse distribution rules ID"))?,
            created_at: value.created_at,
            updated_at: value.updated_at,
            account_id: Uuid::parse_str(&value.account_id)
                .map_err(|_| ConversionError::new("account_id", "Failed to parse account ID"))?,
            earnings_percent: Decimal::from_str(&value.earnings_percent).map_err(|_| {
                ConversionError::new("earnings_percent", "Failed to parse earnings percentage")
            })?,
            tax_percent: Decimal::from_str(&value.tax_percent).map_err(|_| {
                ConversionError::new("tax_percent", "Failed to parse tax percentage")
            })?,
            reinvestment_percent: Decimal::from_str(&value.reinvestment_percent).map_err(|_| {
                ConversionError::new(
                    "reinvestment_percent",
                    "Failed to parse reinvestment percentage",
                )
            })?,
            minimum_threshold: Decimal::from_str(&value.minimum_threshold).map_err(|_| {
                ConversionError::new("minimum_threshold", "Failed to parse minimum threshold")
            })?,
            configuration_password_hash: value.configuration_password_hash,
            insurance_percent: Decimal::from_str(&value.insurance_percent).map_err(|_| {
                ConversionError::new("insurance_percent", "Failed to parse insurance percentage")
            })?,
        })
    }
}

impl IntoDomainModel<DistributionRules> for DistributionRulesSQLite {
    fn into_domain_model(self) -> Result<DistributionRules, Box<dyn Error>> {
        self.try_into().map_err(Into::into)
    }
}

#[derive(Insertable)]
#[diesel(table_name = distribution_rules)]
#[diesel(treat_none_as_null = true)]
struct NewDistributionRules {
    id: String,
    created_at: NaiveDateTime,
    updated_at: NaiveDateTime,
    account_id: String,
    earnings_percent: String,
    tax_percent: String,
    reinvestment_percent: String,
    minimum_threshold: String,
    configuration_password_hash: String,
    insurance_percent: String,
}

#[derive(Debug, Queryable, Identifiable, AsChangeset, Insertable)]
#[diesel(table_name = distribution_history)]
#[diesel(primary_key(id))]
#[diesel(treat_none_as_null = true)]
pub struct DistributionHistorySQLite {
    pub id: String,
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
    pub source_account_id: String,
    pub trade_id: Option<String>,
    pub original_amount: String,
    pub distribution_date: NaiveDateTime,
    pub earnings_amount: Option<String>,
    pub tax_amount: Option<String>,
    pub reinvestment_amount: Option<String>,
    pub insurance_amount: Option<String>,
}

impl TryFrom<DistributionHistorySQLite> for DistributionHistory {
    type Error = ConversionError;

    fn try_from(value: DistributionHistorySQLite) -> Result<Self, Self::Error> {
        Ok(DistributionHistory {
            id: Uuid::parse_str(&value.id)
                .map_err(|_| ConversionError::new("id", "Failed to parse history ID"))?,
            source_account_id: Uuid::parse_str(&value.source_account_id).map_err(|_| {
                ConversionError::new("source_account_id", "Failed to parse source account ID")
            })?,
            trade_id: value
                .trade_id
                .as_deref()
                .map(Uuid::parse_str)
                .transpose()
                .map_err(|_| ConversionError::new("trade_id", "Failed to parse trade ID"))?,
            original_amount: Decimal::from_str(&value.original_amount).map_err(|_| {
                ConversionError::new("original_amount", "Failed to parse original amount")
            })?,
            distribution_date: value.distribution_date,
            earnings_amount: value
                .earnings_amount
                .as_deref()
                .map(Decimal::from_str)
                .transpose()
                .map_err(|_| {
                    ConversionError::new("earnings_amount", "Failed to parse earnings amount")
                })?,
            tax_amount: value
                .tax_amount
                .as_deref()
                .map(Decimal::from_str)
                .transpose()
                .map_err(|_| ConversionError::new("tax_amount", "Failed to parse tax amount"))?,
            reinvestment_amount: value
                .reinvestment_amount
                .as_deref()
                .map(Decimal::from_str)
                .transpose()
                .map_err(|_| {
                    ConversionError::new(
                        "reinvestment_amount",
                        "Failed to parse reinvestment amount",
                    )
                })?,
            insurance_amount: value
                .insurance_amount
                .as_deref()
                .map(Decimal::from_str)
                .transpose()
                .map_err(|_| {
                    ConversionError::new("insurance_amount", "Failed to parse insurance amount")
                })?,
            created_at: value.created_at,
            updated_at: value.updated_at,
        })
    }
}

impl IntoDomainModel<DistributionHistory> for DistributionHistorySQLite {
    fn into_domain_model(self) -> Result<DistributionHistory, Box<dyn Error>> {
        self.try_into().map_err(Into::into)
    }
}

#[derive(Insertable)]
#[diesel(table_name = distribution_history)]
#[diesel(treat_none_as_null = true)]
struct NewDistributionHistory {
    id: String,
    created_at: NaiveDateTime,
    updated_at: NaiveDateTime,
    source_account_id: String,
    trade_id: Option<String>,
    original_amount: String,
    distribution_date: NaiveDateTime,
    earnings_amount: Option<String>,
    tax_amount: Option<String>,
    reinvestment_amount: Option<String>,
    insurance_amount: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::SqliteDatabase;
    use chrono::Duration;
    use diesel_migrations::*;
    use model::{
        Account, AccountType, Currency, DatabaseFactory, DistributionExecutionLeg,
        DistributionExecutionPlan, DraftTrade, Environment, OrderAction, OrderCategory,
        TradeCategory, TradingVehicleCategory, TransactionCategory,
    };
    use rust_decimal_macros::dec;

    pub const MIGRATIONS: EmbeddedMigrations = embed_migrations!();

    fn setup_connection() -> SqliteConnection {
        let mut connection = SqliteConnection::establish(":memory:").unwrap();
        connection.run_pending_migrations(MIGRATIONS).unwrap();
        connection.begin_test_transaction().unwrap();
        connection
    }

    fn database_with_connection() -> (SqliteDatabase, Arc<Mutex<SqliteConnection>>) {
        let connection = Arc::new(Mutex::new(setup_connection()));
        (SqliteDatabase::new_from(connection.clone()), connection)
    }

    fn poisoned_distribution_db() -> DistributionDB {
        let connection = Arc::new(Mutex::new(setup_connection()));
        let poisoned_connection = Arc::clone(&connection);
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _guard = poisoned_connection
                .lock()
                .expect("connection lock should be acquired before poisoning");
            std::panic::resume_unwind(Box::new("poison distribution connection lock"));
        }));
        DistributionDB { connection }
    }

    fn assert_connection_lock_error<T>(result: Result<T, Box<dyn Error>>) {
        assert!(result.is_err());
        let error = result.err().expect("operation should fail");
        assert!(error
            .to_string()
            .contains("failed to acquire distribution database connection lock"));
    }

    fn create_account(database: &SqliteDatabase, name: &str) -> Account {
        database
            .account_write()
            .create_with_hierarchy(
                name,
                name,
                Environment::Paper,
                dec!(0),
                dec!(0),
                AccountType::Primary,
                None,
            )
            .expect("account should be created")
    }

    fn create_child_account(database: &SqliteDatabase, source: &Account, name: &str) -> Account {
        database
            .account_write()
            .create_with_hierarchy(
                name,
                name,
                Environment::Paper,
                dec!(0),
                dec!(0),
                AccountType::Earnings,
                Some(source.id),
            )
            .expect("child account should be created")
    }

    fn one_leg_plan(
        source_account_id: Uuid,
        destination_account_id: Uuid,
        amount: Decimal,
    ) -> DistributionExecutionPlan {
        DistributionExecutionPlan {
            source_account_id,
            currency: Currency::USD,
            trade_id: None,
            original_amount: amount,
            distribution_date: Utc::now().naive_utc(),
            legs: vec![DistributionExecutionLeg {
                to_account_id: destination_account_id,
                amount,
                withdrawal_category: TransactionCategory::Withdrawal,
                deposit_category: TransactionCategory::Deposit,
                forced_withdrawal_tx_id: None,
                forced_deposit_tx_id: None,
            }],
            earnings_amount: Some(amount),
            tax_amount: None,
            reinvestment_amount: None,
            insurance_amount: None,
        }
    }

    fn create_trade_for_distribution_history(
        database: &SqliteDatabase,
        account: &Account,
    ) -> model::Trade {
        let vehicle = database
            .trading_vehicle_write()
            .create_trading_vehicle(
                "DISTHIST",
                Some("US000000DIST"),
                &TradingVehicleCategory::Stock,
                "alpaca",
            )
            .expect("trading vehicle should be created");
        let stop = database
            .order_write()
            .create(
                &vehicle,
                dec!(10),
                dec!(90),
                &Currency::USD,
                &OrderAction::Sell,
                &OrderCategory::Stop,
            )
            .expect("stop order should be created");
        let entry = database
            .order_write()
            .create(
                &vehicle,
                dec!(10),
                dec!(100),
                &Currency::USD,
                &OrderAction::Buy,
                &OrderCategory::Limit,
            )
            .expect("entry order should be created");
        let target = database
            .order_write()
            .create(
                &vehicle,
                dec!(10),
                dec!(120),
                &Currency::USD,
                &OrderAction::Sell,
                &OrderCategory::Limit,
            )
            .expect("target order should be created");
        let draft = DraftTrade {
            account: account.clone(),
            trading_vehicle: vehicle,
            quantity: 10.into(),
            currency: Currency::USD,
            category: TradeCategory::Long,
            thesis: Some("distribution history trade link".to_string()),
            sector: Some("technology".to_string()),
            asset_class: Some("equity".to_string()),
            context: Some("unit test".to_string()),
        };
        database
            .trade_write()
            .create_trade(draft, &stop, &entry, &target)
            .expect("trade should be created")
    }

    #[test]
    fn debug_representation_hides_connection_internals() {
        let db = DistributionDB {
            connection: Arc::new(Mutex::new(
                SqliteConnection::establish(":memory:").expect("in-memory sqlite connection"),
            )),
        };

        assert_eq!(
            format!("{db:?}"),
            "DistributionDB { connection: \"Arc<Mutex<SqliteConnection>>\" }"
        );
    }

    #[test]
    fn distribution_db_methods_return_errors_when_connection_lock_is_poisoned() {
        let mut db = poisoned_distribution_db();
        let source_account_id = Uuid::new_v4();
        let destination_account_id = Uuid::new_v4();
        let plan = one_leg_plan(source_account_id, destination_account_id, dec!(10));

        assert_connection_lock_error(db.for_account(source_account_id));
        assert_connection_lock_error(db.history_for_account(source_account_id));
        assert_connection_lock_error(db.create_or_update(
            source_account_id,
            dec!(0.40),
            dec!(0.30),
            dec!(0.30),
            dec!(100),
            "hash",
        ));
        assert_connection_lock_error(db.create_history(
            source_account_id,
            None,
            dec!(100),
            Utc::now().naive_utc(),
            Some(dec!(40)),
            Some(dec!(30)),
            Some(dec!(30)),
        ));
        assert_connection_lock_error(db.execute_distribution_plan_atomic(&plan));
    }

    fn assert_no_distribution_writes(database: &SqliteDatabase, source: &Account, child: &Account) {
        let source_transactions = database
            .transaction_read()
            .all_transactions(source.id, &Currency::USD)
            .expect("source transactions should be readable");
        let child_transactions = database
            .transaction_read()
            .all_transactions(child.id, &Currency::USD)
            .expect("child transactions should be readable");
        let history = database
            .distribution_read()
            .history_for_account(source.id)
            .expect("distribution history should be readable");

        assert!(source_transactions.is_empty());
        assert!(child_transactions.is_empty());
        assert!(history.is_empty());
    }

    fn rules_row() -> DistributionRulesSQLite {
        let now = Utc::now().naive_utc();
        DistributionRulesSQLite {
            id: Uuid::new_v4().to_string(),
            created_at: now,
            updated_at: now,
            account_id: Uuid::new_v4().to_string(),
            earnings_percent: dec!(0.40).to_string(),
            tax_percent: dec!(0.30).to_string(),
            reinvestment_percent: dec!(0.30).to_string(),
            insurance_percent: Decimal::ZERO.to_string(),
            minimum_threshold: dec!(100).to_string(),
            configuration_password_hash: "hash".to_string(),
        }
    }

    fn assert_rules_conversion_error(row: DistributionRulesSQLite, field: &str) {
        let err = DistributionRules::try_from(row).expect_err("conversion should fail");
        assert!(err.to_string().contains(field));
    }

    fn history_row() -> DistributionHistorySQLite {
        let now = Utc::now().naive_utc();
        DistributionHistorySQLite {
            id: Uuid::new_v4().to_string(),
            created_at: now,
            updated_at: now,
            source_account_id: Uuid::new_v4().to_string(),
            trade_id: None,
            original_amount: dec!(1000).to_string(),
            distribution_date: now,
            earnings_amount: Some(dec!(400).to_string()),
            tax_amount: Some(dec!(300).to_string()),
            reinvestment_amount: Some(dec!(300).to_string()),
            insurance_amount: None,
        }
    }

    fn assert_history_conversion_error(row: DistributionHistorySQLite, field: &str) {
        let err = DistributionHistory::try_from(row).expect_err("conversion should fail");
        assert!(err.to_string().contains(field));
    }

    #[test]
    fn distribution_rules_upsert_read_and_missing_account_are_deterministic() {
        let database = SqliteDatabase::new_in_memory();
        let account = create_account(&database, "distribution-rules-upsert");

        let missing_error = database
            .distribution_read()
            .for_account(Uuid::new_v4())
            .expect_err("missing account rules should fail");
        assert!(missing_error
            .downcast_ref::<DistributionRulesNotFound>()
            .is_some());

        let created = database
            .distribution_write()
            .create_or_update(
                account.id,
                dec!(0.40),
                dec!(0.30),
                dec!(0.30),
                dec!(100),
                "first-hash",
            )
            .expect("rules should be created");

        let updated = database
            .distribution_write()
            .create_or_update(
                account.id,
                dec!(0.50),
                dec!(0.20),
                dec!(0.30),
                dec!(250),
                "second-hash",
            )
            .expect("rules should be updated");

        assert_eq!(updated.id, created.id);
        assert_eq!(updated.account_id, account.id);
        assert_eq!(updated.earnings_percent, dec!(0.50));
        assert_eq!(updated.tax_percent, dec!(0.20));
        assert_eq!(updated.reinvestment_percent, dec!(0.30));
        assert_eq!(updated.minimum_threshold, dec!(250));
        assert_eq!(updated.configuration_password_hash, "second-hash");

        let fetched = database
            .distribution_read()
            .for_account(account.id)
            .expect("rules should be readable after update");
        assert_eq!(fetched, updated);
    }

    #[test]
    fn distribution_rules_read_reports_database_errors() {
        let (database, connection) = database_with_connection();
        diesel::sql_query("DROP TABLE distribution_rules")
            .execute(
                &mut *connection
                    .lock()
                    .expect("connection lock should be available"),
            )
            .expect("distribution_rules table should drop");

        let error = database
            .distribution_read()
            .for_account(Uuid::new_v4())
            .expect_err("missing table should fail distribution rules read");

        assert!(error.to_string().contains("distribution_rules"));
    }

    #[test]
    fn distribution_rules_create_reports_foreign_key_errors() {
        let database = SqliteDatabase::new_in_memory();

        let error = database
            .distribution_write()
            .create_or_update(
                Uuid::new_v4(),
                dec!(0.40),
                dec!(0.30),
                dec!(0.30),
                dec!(100),
                "hash",
            )
            .expect_err("unknown account should fail distribution rules insert");

        assert!(error.to_string().contains("FOREIGN KEY"));
    }

    #[test]
    fn distribution_rules_upsert_reports_lookup_and_update_database_errors() {
        let (database, connection) = database_with_connection();
        diesel::sql_query("DROP TABLE distribution_rules")
            .execute(
                &mut *connection
                    .lock()
                    .expect("connection lock should be available"),
            )
            .expect("distribution_rules table should drop");

        let lookup_error = database
            .distribution_write()
            .create_or_update(
                Uuid::new_v4(),
                dec!(0.40),
                dec!(0.30),
                dec!(0.30),
                dec!(100),
                "hash",
            )
            .expect_err("missing table should fail distribution rules lookup");
        assert!(lookup_error.to_string().contains("distribution_rules"));

        let (database, connection) = database_with_connection();
        let account = create_account(&database, "distribution-rules-update-error");
        database
            .distribution_write()
            .create_or_update(
                account.id,
                dec!(0.40),
                dec!(0.30),
                dec!(0.30),
                dec!(100),
                "hash",
            )
            .expect("initial rules should be created");

        diesel::sql_query(
            "CREATE TRIGGER fail_distribution_rules_update \
             BEFORE UPDATE ON distribution_rules \
             BEGIN \
             SELECT RAISE(ABORT, 'forced distribution rules update failure'); \
             END",
        )
        .execute(
            &mut *connection
                .lock()
                .expect("connection lock should be available"),
        )
        .expect("update failure trigger should be created");

        let update_error = database
            .distribution_write()
            .create_or_update(
                account.id,
                dec!(0.45),
                dec!(0.25),
                dec!(0.30),
                dec!(150),
                "updated-hash",
            )
            .expect_err("trigger should fail distribution rules update");
        assert!(update_error
            .to_string()
            .contains("forced distribution rules update failure"));
    }

    #[test]
    fn distribution_history_is_read_newest_first_and_preserves_optional_amounts() {
        let database = SqliteDatabase::new_in_memory();
        let account = create_account(&database, "distribution-history-order");
        let older_date = Utc::now().naive_utc();
        let newer_date = older_date
            .checked_add_signed(Duration::days(1))
            .expect("test date should be representable");

        let older = database
            .distribution_write()
            .create_history(
                account.id,
                None,
                dec!(100),
                older_date,
                Some(dec!(40)),
                None,
                Some(dec!(60)),
            )
            .expect("older history should be created");
        let newer = database
            .distribution_write()
            .create_history(
                account.id,
                None,
                dec!(200),
                newer_date,
                None,
                Some(dec!(75)),
                Some(dec!(125)),
            )
            .expect("newer history should be created");

        let history = database
            .distribution_read()
            .history_for_account(account.id)
            .expect("history should be readable");
        let mut entries = history.iter();
        let first = entries.next().expect("newest history should exist");
        let second = entries.next().expect("older history should exist");
        assert!(entries.next().is_none());

        assert_eq!(first.id, newer.id);
        assert_eq!(first.earnings_amount, None);
        assert_eq!(first.tax_amount, Some(dec!(75)));
        assert_eq!(second.id, older.id);
        assert_eq!(second.earnings_amount, Some(dec!(40)));
        assert_eq!(second.tax_amount, None);
    }

    #[test]
    fn distribution_history_read_reports_database_errors() {
        let (database, connection) = database_with_connection();
        diesel::sql_query("DROP TABLE distribution_history")
            .execute(
                &mut *connection
                    .lock()
                    .expect("connection lock should be available"),
            )
            .expect("distribution_history table should drop");

        let error = database
            .distribution_read()
            .history_for_account(Uuid::new_v4())
            .expect_err("missing table should fail distribution history read");

        assert!(error.to_string().contains("distribution_history"));
    }

    #[test]
    fn distribution_history_create_reports_foreign_key_errors() {
        let database = SqliteDatabase::new_in_memory();

        let error = database
            .distribution_write()
            .create_history(
                Uuid::new_v4(),
                None,
                dec!(100),
                Utc::now().naive_utc(),
                Some(dec!(40)),
                Some(dec!(30)),
                Some(dec!(30)),
            )
            .expect_err("unknown source account should fail distribution history insert");

        assert!(error.to_string().contains("FOREIGN KEY"));
    }

    #[test]
    fn distribution_history_create_round_trips_optional_trade_id() {
        let database = SqliteDatabase::new_in_memory();
        let account = create_account(&database, "distribution-history-trade");
        let trade = create_trade_for_distribution_history(&database, &account);
        let distribution_date = Utc::now().naive_utc();

        let created = database
            .distribution_write()
            .create_history(
                account.id,
                Some(trade.id),
                dec!(100),
                distribution_date,
                Some(dec!(40)),
                Some(dec!(30)),
                Some(dec!(30)),
            )
            .expect("distribution history should be created");

        assert_eq!(created.trade_id, Some(trade.id));
        assert_eq!(created.original_amount, dec!(100));

        let persisted = database
            .distribution_read()
            .history_for_account(account.id)
            .expect("distribution history should be readable")
            .into_iter()
            .next()
            .expect("history row should exist");
        assert_eq!(persisted.id, created.id);
        assert_eq!(persisted.trade_id, Some(trade.id));
        assert_eq!(persisted.distribution_date, distribution_date);
    }

    #[test]
    fn execute_distribution_plan_rejects_empty_and_non_positive_legs_without_writes() {
        let database = SqliteDatabase::new_in_memory();
        let source = create_account(&database, "distribution-invalid-source");
        let child = create_child_account(&database, &source, "distribution-invalid-child");

        let mut empty_plan = one_leg_plan(source.id, child.id, dec!(10));
        empty_plan.legs.clear();
        let empty_error = database
            .distribution_write()
            .execute_distribution_plan_atomic(&empty_plan)
            .expect_err("empty plan should fail");
        assert!(empty_error
            .to_string()
            .contains("at least one transfer leg"));
        assert_no_distribution_writes(&database, &source, &child);

        let zero_plan = one_leg_plan(source.id, child.id, Decimal::ZERO);
        let zero_error = database
            .distribution_write()
            .execute_distribution_plan_atomic(&zero_plan)
            .expect_err("zero amount leg should fail");
        assert!(zero_error.to_string().contains("must be positive"));
        assert_no_distribution_writes(&database, &source, &child);
    }

    #[test]
    fn execute_distribution_plan_rolls_back_when_transfer_leg_insert_fails() {
        let database = SqliteDatabase::new_in_memory();
        let source = create_account(&database, "distribution-transfer-fail-source");
        let child = create_child_account(&database, &source, "distribution-transfer-fail-child");

        let unknown_source_plan = one_leg_plan(Uuid::new_v4(), child.id, dec!(10));
        let error = database
            .distribution_write()
            .execute_distribution_plan_atomic(&unknown_source_plan)
            .expect_err("unknown source account should fail withdrawal insert");
        assert!(error.to_string().contains("FOREIGN KEY"));
        assert_no_distribution_writes(&database, &source, &child);

        let unknown_destination_plan = one_leg_plan(source.id, Uuid::new_v4(), dec!(10));
        let error = database
            .distribution_write()
            .execute_distribution_plan_atomic(&unknown_destination_plan)
            .expect_err("unknown destination account should fail deposit insert");
        assert!(error.to_string().contains("FOREIGN KEY"));
        assert_no_distribution_writes(&database, &source, &child);
    }

    #[test]
    #[allow(clippy::cognitive_complexity, clippy::too_many_lines)]
    fn execute_distribution_plan_writes_multiple_legs_and_full_history_amounts() {
        let database = SqliteDatabase::new_in_memory();
        let source = create_account(&database, "distribution-multi-leg-source");
        let earnings = create_child_account(&database, &source, "distribution-multi-leg-earnings");
        let tax = create_child_account(&database, &source, "distribution-multi-leg-tax");
        let reinvestment =
            create_child_account(&database, &source, "distribution-multi-leg-reinvestment");
        let distribution_date = Utc::now().naive_utc();
        let earnings_deposit_id = Uuid::new_v4();
        let tax_deposit_id = Uuid::new_v4();
        let reinvestment_deposit_id = Uuid::new_v4();

        let plan = DistributionExecutionPlan {
            source_account_id: source.id,
            currency: Currency::USD,
            trade_id: None,
            original_amount: dec!(100),
            distribution_date,
            legs: vec![
                DistributionExecutionLeg {
                    to_account_id: earnings.id,
                    amount: dec!(40),
                    withdrawal_category: TransactionCategory::WithdrawalEarnings,
                    deposit_category: TransactionCategory::Deposit,
                    forced_withdrawal_tx_id: Some(Uuid::new_v4()),
                    forced_deposit_tx_id: Some(earnings_deposit_id),
                },
                DistributionExecutionLeg {
                    to_account_id: tax.id,
                    amount: dec!(30),
                    withdrawal_category: TransactionCategory::WithdrawalTax,
                    deposit_category: TransactionCategory::Deposit,
                    forced_withdrawal_tx_id: Some(Uuid::new_v4()),
                    forced_deposit_tx_id: Some(tax_deposit_id),
                },
                DistributionExecutionLeg {
                    to_account_id: reinvestment.id,
                    amount: dec!(30),
                    withdrawal_category: TransactionCategory::Withdrawal,
                    deposit_category: TransactionCategory::Deposit,
                    forced_withdrawal_tx_id: Some(Uuid::new_v4()),
                    forced_deposit_tx_id: Some(reinvestment_deposit_id),
                },
            ],
            earnings_amount: Some(dec!(40)),
            tax_amount: Some(dec!(30)),
            reinvestment_amount: Some(dec!(30)),
            insurance_amount: None,
        };

        let deposit_ids = database
            .distribution_write()
            .execute_distribution_plan_atomic(&plan)
            .expect("multi-leg distribution should succeed");
        assert_eq!(
            deposit_ids,
            vec![earnings_deposit_id, tax_deposit_id, reinvestment_deposit_id]
        );

        let source_transactions = database
            .transaction_read()
            .all_transactions(source.id, &Currency::USD)
            .expect("source transactions should be readable");
        assert_eq!(source_transactions.len(), 3);
        assert!(source_transactions
            .iter()
            .all(|transaction| transaction.amount < Decimal::ZERO));

        let earnings_transaction = database
            .transaction_read()
            .all_transactions(earnings.id, &Currency::USD)
            .expect("earnings transactions should be readable")
            .into_iter()
            .next()
            .expect("earnings deposit should be written");
        assert_eq!(earnings_transaction.id, earnings_deposit_id);
        assert_eq!(earnings_transaction.amount, dec!(40));

        let tax_transaction = database
            .transaction_read()
            .all_transactions(tax.id, &Currency::USD)
            .expect("tax transactions should be readable")
            .into_iter()
            .next()
            .expect("tax deposit should be written");
        assert_eq!(tax_transaction.id, tax_deposit_id);
        assert_eq!(tax_transaction.amount, dec!(30));

        let reinvestment_transaction = database
            .transaction_read()
            .all_transactions(reinvestment.id, &Currency::USD)
            .expect("reinvestment transactions should be readable")
            .into_iter()
            .next()
            .expect("reinvestment deposit should be written");
        assert_eq!(reinvestment_transaction.id, reinvestment_deposit_id);
        assert_eq!(reinvestment_transaction.amount, dec!(30));

        let history = database
            .distribution_read()
            .history_for_account(source.id)
            .expect("distribution history should be readable")
            .into_iter()
            .next()
            .expect("distribution history should be written");
        assert_eq!(history.source_account_id, source.id);
        assert_eq!(history.original_amount, dec!(100));
        assert_eq!(history.distribution_date, distribution_date);
        assert_eq!(history.earnings_amount, Some(dec!(40)));
        assert_eq!(history.tax_amount, Some(dec!(30)));
        assert_eq!(history.reinvestment_amount, Some(dec!(30)));
        assert_eq!(history.insurance_amount, None);
    }

    #[test]
    fn execute_distribution_plan_rolls_back_when_history_insert_fails() {
        let database = SqliteDatabase::new_in_memory();
        let source = create_account(&database, "distribution-history-fail-source");
        let child = create_child_account(&database, &source, "distribution-history-fail-child");
        let mut plan = one_leg_plan(source.id, child.id, dec!(10));
        plan.trade_id = Some(Uuid::new_v4());

        let error = database
            .distribution_write()
            .execute_distribution_plan_atomic(&plan)
            .expect_err("unknown trade should fail distribution history insert");

        assert!(error.to_string().contains("FOREIGN KEY"));
        assert_no_distribution_writes(&database, &source, &child);
    }

    #[test]
    fn distribution_rules_sqlite_conversion_reports_corrupt_fields() {
        let mut row = rules_row();
        row.id = "not-a-uuid".to_string();
        assert_rules_conversion_error(row, "id");

        let mut row = rules_row();
        row.account_id = "not-a-uuid".to_string();
        assert_rules_conversion_error(row, "account_id");

        let mut row = rules_row();
        row.earnings_percent = "not-decimal".to_string();
        assert_rules_conversion_error(row, "earnings_percent");

        let mut row = rules_row();
        row.tax_percent = "not-decimal".to_string();
        assert_rules_conversion_error(row, "tax_percent");

        let mut row = rules_row();
        row.reinvestment_percent = "not-decimal".to_string();
        assert_rules_conversion_error(row, "reinvestment_percent");

        let mut row = rules_row();
        row.insurance_percent = "not-decimal".to_string();
        assert_rules_conversion_error(row, "insurance_percent");

        let mut row = rules_row();
        row.minimum_threshold = "not-decimal".to_string();
        assert_rules_conversion_error(row, "minimum_threshold");
    }

    #[test]
    fn distribution_history_sqlite_conversion_reports_corrupt_fields() {
        let mut row = history_row();
        row.id = "not-a-uuid".to_string();
        assert_history_conversion_error(row, "id");

        let mut row = history_row();
        row.source_account_id = "not-a-uuid".to_string();
        assert_history_conversion_error(row, "source_account_id");

        let mut row = history_row();
        row.trade_id = Some("not-a-uuid".to_string());
        assert_history_conversion_error(row, "trade_id");

        let mut row = history_row();
        row.original_amount = "not-decimal".to_string();
        assert_history_conversion_error(row, "original_amount");

        let mut row = history_row();
        row.earnings_amount = Some("not-decimal".to_string());
        assert_history_conversion_error(row, "earnings_amount");

        let mut row = history_row();
        row.tax_amount = Some("not-decimal".to_string());
        assert_history_conversion_error(row, "tax_amount");

        let mut row = history_row();
        row.reinvestment_amount = Some("not-decimal".to_string());
        assert_history_conversion_error(row, "reinvestment_amount");

        let mut row = history_row();
        row.insurance_amount = Some("not-decimal".to_string());
        assert_history_conversion_error(row, "insurance_amount");
    }
}
