use crate::error::{ConversionError, IntoDomainModel};
use crate::schema::distribution_rules;
use chrono::{NaiveDateTime, Utc};
use diesel::prelude::*;
use model::{DistributionRead, DistributionRules, DistributionRulesNotFound, DistributionWrite};
use rust_decimal::Decimal;
use std::error::Error;
use std::str::FromStr;
use std::sync::Arc;
use std::sync::Mutex;
use tracing::error;
use uuid::Uuid;

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

impl DistributionRead for DistributionDB {
    fn for_account(&mut self, account_id: Uuid) -> Result<DistributionRules, Box<dyn Error>> {
        let connection: &mut SqliteConnection = &mut self.connection.lock().unwrap_or_else(|e| {
            eprintln!("Failed to acquire connection lock: {e}");
            std::process::exit(1);
        });

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
            minimum_threshold: minimum_threshold.to_string(),
            configuration_password_hash: configuration_password_hash.to_string(),
        };

        let connection: &mut SqliteConnection = &mut self.connection.lock().unwrap_or_else(|e| {
            eprintln!("Failed to acquire connection lock: {e}");
            std::process::exit(1);
        });

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
}
