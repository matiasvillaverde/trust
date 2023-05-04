use crate::schema::rules;
use chrono::{NaiveDateTime, Utc};
use diesel::prelude::*;
use std::error::Error;
use tracing::error;
use trust_model::{Account, Rule, RuleLevel, RuleName};
use uuid::Uuid;

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

        let inserted_rule = diesel::insert_into(rules::table)
            .values(&new_rule)
            .get_result::<RuleSQLite>(connection)
            .map(|rule| rule.domain_model())
            .map_err(|error| {
                error!("Error creating rule: {:?}", error);
                error
            })?;
        Ok(inserted_rule)
    }

    pub fn read_all(
        connection: &mut SqliteConnection,
        account_id: Uuid,
    ) -> Result<Vec<Rule>, Box<dyn Error>> {
        let rules = rules::table
            .filter(rules::account_id.eq(account_id.to_string()))
            .filter(rules::deleted_at.is_null())
            .filter(rules::active.eq(true))
            .load::<RuleSQLite>(connection)
            .map(|rules| {
                rules
                    .into_iter()
                    .map(|rule| rule.domain_model())
                    .collect::<Vec<Rule>>()
            })
            .map_err(|error| {
                error!("Error reading rules: {:?}", error);
                error
            })?;
        Ok(rules)
    }

    pub fn make_inactive(
        connection: &mut SqliteConnection,
        rule: &Rule,
    ) -> Result<Rule, Box<dyn Error>> {
        let rule = diesel::update(rules::table)
            .filter(rules::id.eq(rule.id.to_string()))
            .set(rules::active.eq(false))
            .get_result::<RuleSQLite>(connection)
            .map(|rule| rule.domain_model())
            .map_err(|error| {
                error!("Error making rule inactive: {:?}", error);
                error
            })?;
        Ok(rule)
    }

    pub fn read_for_account_with_name(
        connection: &mut SqliteConnection,
        account_id: Uuid,
        name: &RuleName,
    ) -> Result<Rule, Box<dyn Error>> {
        let rule = rules::table
            .filter(rules::account_id.eq(account_id.to_string()))
            .filter(rules::deleted_at.is_null())
            .filter(rules::active.eq(true))
            .filter(rules::name.eq(name.to_string()))
            .first::<RuleSQLite>(connection)
            .map_err(|error| {
                error!("Error reading rule: {:?}", error);
                error
            })
            .map(|rule| rule.domain_model())?;
        Ok(rule)
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

impl RuleSQLite {
    fn domain_model(self) -> Rule {
        use std::str::FromStr;
        let name =
            RuleName::parse(&self.name, self.risk as u32).expect("Failed to parse rule name");
        Rule {
            id: Uuid::parse_str(&self.id).unwrap(),
            created_at: self.created_at,
            updated_at: self.updated_at,
            deleted_at: self.deleted_at,
            name,
            description: self.description,
            priority: self.priority as u32,
            level: RuleLevel::from_str(&self.level).unwrap(),
            account_id: Uuid::parse_str(&self.account_id).unwrap(),
            active: self.active,
        }
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
