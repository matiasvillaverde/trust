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
use std::sync::Arc;
use std::sync::Mutex;
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

    fn history_for_account(
        &mut self,
        account_id: Uuid,
    ) -> Result<Vec<DistributionHistory>, Box<dyn Error>> {
        let connection: &mut SqliteConnection = &mut self.connection.lock().unwrap_or_else(|e| {
            eprintln!("Failed to acquire connection lock: {e}");
            std::process::exit(1);
        });

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
        let connection: &mut SqliteConnection = &mut self.connection.lock().unwrap_or_else(|e| {
            eprintln!("Failed to acquire connection lock: {e}");
            std::process::exit(1);
        });

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

        let connection: &mut SqliteConnection = &mut self.connection.lock().unwrap_or_else(|e| {
            eprintln!("Failed to acquire connection lock: {e}");
            std::process::exit(1);
        });

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
}
