use crate::error::{ConversionError, IntoDomainModel, IntoDomainModels};
use crate::schema::rules;
use chrono::{NaiveDateTime, Utc};
use diesel::prelude::*;
use model::{Account, Rule, RuleLevel, RuleName};
use std::error::Error;
use std::str::FromStr;
use tracing::error;
use uuid::Uuid;

/// Worker for handling rule database operations
#[derive(Debug)]
pub struct WorkerRule;
impl WorkerRule {
    pub fn create(
        connection: &mut SqliteConnection,
        name: &RuleName,
        description: &str,
        priority: u32,
        level: &RuleLevel,
        account: &Account,
    ) -> Result<Rule, Box<dyn Error>> {
        let uuid = Uuid::new_v4().to_string();
        let now = Utc::now().naive_utc();

        let new_rule = NewRule {
            id: uuid,
            created_at: now,
            updated_at: now,
            deleted_at: None,
            name: name.to_string(),
            risk: name.risk() as i32,
            description: description.to_string(),
            priority: priority as i32,
            level: level.to_string(),
            account_id: account.id.to_string(),
            active: true,
        };

        diesel::insert_into(rules::table)
            .values(&new_rule)
            .get_result::<RuleSQLite>(connection)
            .map_err(|error| {
                error!("Error creating rule: {:?}", error);
                error
            })?
            .into_domain_model()
    }

    pub fn read_all(
        connection: &mut SqliteConnection,
        account_id: Uuid,
    ) -> Result<Vec<Rule>, Box<dyn Error>> {
        rules::table
            .filter(rules::account_id.eq(account_id.to_string()))
            .filter(rules::deleted_at.is_null())
            .filter(rules::active.eq(true))
            .load::<RuleSQLite>(connection)
            .map_err(|error| {
                error!("Error reading rules: {:?}", error);
                error
            })?
            .into_domain_models()
    }

    pub fn make_inactive(
        connection: &mut SqliteConnection,
        rule: &Rule,
    ) -> Result<Rule, Box<dyn Error>> {
        diesel::update(rules::table)
            .filter(rules::id.eq(rule.id.to_string()))
            .set(rules::active.eq(false))
            .get_result::<RuleSQLite>(connection)
            .map_err(|error| {
                error!("Error making rule inactive: {:?}", error);
                error
            })?
            .into_domain_model()
    }

    pub fn read_for_account_with_name(
        connection: &mut SqliteConnection,
        account_id: Uuid,
        name: &RuleName,
    ) -> Result<Rule, Box<dyn Error>> {
        rules::table
            .filter(rules::account_id.eq(account_id.to_string()))
            .filter(rules::deleted_at.is_null())
            .filter(rules::active.eq(true))
            .filter(rules::name.eq(name.to_string()))
            .first::<RuleSQLite>(connection)
            .map_err(|error| {
                error!("Error reading rule: {:?}", error);
                error
            })?
            .into_domain_model()
    }
}

#[derive(Queryable, Identifiable, AsChangeset, Insertable)]
#[diesel(table_name = rules)]
struct RuleSQLite {
    id: String,
    created_at: NaiveDateTime,
    updated_at: NaiveDateTime,
    deleted_at: Option<NaiveDateTime>,
    name: String,
    risk: i32,
    description: String,
    priority: i32,
    level: String,
    account_id: String,
    active: bool,
}

impl TryFrom<RuleSQLite> for Rule {
    type Error = ConversionError;

    fn try_from(value: RuleSQLite) -> Result<Self, Self::Error> {
        let name = RuleName::parse(&value.name, value.risk as f32)
            .map_err(|_| ConversionError::new("name", "Failed to parse rule name"))?;
        Ok(Rule {
            id: Uuid::parse_str(&value.id)
                .map_err(|_| ConversionError::new("id", "Failed to parse rule ID"))?,
            created_at: value.created_at,
            updated_at: value.updated_at,
            deleted_at: value.deleted_at,
            name,
            description: value.description,
            priority: value.priority as u32,
            level: RuleLevel::from_str(&value.level)
                .map_err(|_| ConversionError::new("level", "Failed to parse rule level"))?,
            account_id: Uuid::parse_str(&value.account_id)
                .map_err(|_| ConversionError::new("account_id", "Failed to parse account ID"))?,
            active: value.active,
        })
    }
}

impl IntoDomainModel<Rule> for RuleSQLite {
    fn into_domain_model(self) -> Result<Rule, Box<dyn Error>> {
        self.try_into().map_err(Into::into)
    }
}
#[derive(Insertable)]
#[diesel(table_name = rules)]
#[diesel(treat_none_as_null = true)]
struct NewRule {
    id: String,
    created_at: NaiveDateTime,
    updated_at: NaiveDateTime,
    deleted_at: Option<NaiveDateTime>,
    name: String,
    risk: i32,
    description: String,
    priority: i32,
    level: String,
    account_id: String,
    active: bool,
}
