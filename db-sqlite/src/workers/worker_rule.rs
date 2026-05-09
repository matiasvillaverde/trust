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
            #[allow(clippy::cast_possible_truncation)]
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
        #[allow(clippy::cast_precision_loss)]
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
            #[allow(clippy::cast_sign_loss)]
            priority: value.priority.max(0) as u32,
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

#[cfg(test)]
mod tests {
    use super::*;
    use diesel::Connection;
    use diesel_migrations::*;

    pub const MIGRATIONS: EmbeddedMigrations = embed_migrations!();

    fn setup_connection() -> SqliteConnection {
        let mut connection = SqliteConnection::establish(":memory:").unwrap();
        connection.run_pending_migrations(MIGRATIONS).unwrap();
        connection.begin_test_transaction().unwrap();
        connection
    }

    fn test_account() -> Account {
        Account {
            id: Uuid::new_v4(),
            ..Default::default()
        }
    }

    fn base_sqlite_rule() -> RuleSQLite {
        let now = Utc::now().naive_utc();
        RuleSQLite {
            id: Uuid::new_v4().to_string(),
            created_at: now,
            updated_at: now,
            deleted_at: None,
            name: RuleName::RiskPerTrade(2.0).to_string(),
            risk: 2,
            description: "risk cap".to_string(),
            priority: 10,
            level: RuleLevel::Warning.to_string(),
            account_id: Uuid::new_v4().to_string(),
            active: true,
        }
    }

    fn assert_conversion_error(row: RuleSQLite, field: &str) {
        let error = Rule::try_from(row).expect_err("corrupt rule row must fail conversion");
        assert!(error.to_string().contains(field));
    }

    fn assert_error_mentions(error: Box<dyn Error>, expected: &str) {
        let message = error.to_string();
        assert!(
            message.contains(expected),
            "expected error to mention {expected:?}, got {message:?}"
        );
    }

    fn drop_rules_table(connection: &mut SqliteConnection) {
        diesel::sql_query("DROP TABLE rules")
            .execute(connection)
            .expect("rules table should be dropped");
    }

    fn domain_rule() -> Rule {
        Rule::try_from(base_sqlite_rule()).expect("base rule should convert")
    }

    #[test]
    fn create_read_and_make_inactive_filters_rule_queries() {
        let mut conn = setup_connection();
        let account = test_account();
        let other_account = test_account();
        let per_trade = RuleName::RiskPerTrade(2.0);
        let per_month = RuleName::RiskPerMonth(8.0);

        let trade_rule = WorkerRule::create(
            &mut conn,
            &per_trade,
            "trade risk",
            10,
            &RuleLevel::Error,
            &account,
        )
        .expect("trade rule should be created");
        let month_rule = WorkerRule::create(
            &mut conn,
            &per_month,
            "monthly risk",
            20,
            &RuleLevel::Warning,
            &account,
        )
        .expect("month rule should be created");
        let other_rule = WorkerRule::create(
            &mut conn,
            &per_trade,
            "other account rule",
            30,
            &RuleLevel::Advice,
            &other_account,
        )
        .expect("other account rule should be created");

        let read = WorkerRule::read_all(&mut conn, account.id).expect("rules should read");
        assert_eq!(read.len(), 2);
        assert!(read.iter().any(|rule| rule.id == trade_rule.id));
        assert!(read.iter().any(|rule| rule.id == month_rule.id));
        assert!(!read.iter().any(|rule| rule.id == other_rule.id));

        let named = WorkerRule::read_for_account_with_name(&mut conn, account.id, &per_trade)
            .expect("named active rule should read");
        assert_eq!(named.id, trade_rule.id);
        assert_eq!(named.level, RuleLevel::Error);

        let inactive =
            WorkerRule::make_inactive(&mut conn, &trade_rule).expect("rule should deactivate");
        assert!(!inactive.active);

        let active = WorkerRule::read_all(&mut conn, account.id).expect("active rules should read");
        assert_eq!(active.len(), 1);
        assert_eq!(active.first().expect("one active rule").id, month_rule.id);
        assert!(WorkerRule::read_for_account_with_name(&mut conn, account.id, &per_trade).is_err());
    }

    #[test]
    fn rule_sqlite_conversion_clamps_negative_priority() {
        let rule = Rule::try_from(RuleSQLite {
            priority: -10,
            ..base_sqlite_rule()
        })
        .expect("negative priority is clamped");

        assert_eq!(rule.priority, 0);
        assert_eq!(rule.name, RuleName::RiskPerTrade(2.0));
    }

    #[test]
    fn rule_sqlite_conversion_reports_corrupt_required_fields() {
        assert_conversion_error(
            RuleSQLite {
                id: "not-a-uuid".to_string(),
                ..base_sqlite_rule()
            },
            "id",
        );
        assert_conversion_error(
            RuleSQLite {
                name: "not-a-rule".to_string(),
                ..base_sqlite_rule()
            },
            "name",
        );
        assert_conversion_error(
            RuleSQLite {
                level: "critical".to_string(),
                ..base_sqlite_rule()
            },
            "level",
        );
        assert_conversion_error(
            RuleSQLite {
                account_id: "not-a-uuid".to_string(),
                ..base_sqlite_rule()
            },
            "account_id",
        );
    }

    #[test]
    fn read_all_filters_soft_deleted_rules() {
        let mut conn = setup_connection();
        let account = test_account();
        let active = WorkerRule::create(
            &mut conn,
            &RuleName::RiskPerTrade(2.0),
            "active",
            10,
            &RuleLevel::Error,
            &account,
        )
        .expect("active rule should be created");
        let deleted = WorkerRule::create(
            &mut conn,
            &RuleName::RiskPerMonth(6.0),
            "deleted",
            20,
            &RuleLevel::Warning,
            &account,
        )
        .expect("deleted rule should be created");

        diesel::update(rules::table.filter(rules::id.eq(deleted.id.to_string())))
            .set(rules::deleted_at.eq(Some(Utc::now().naive_utc())))
            .execute(&mut conn)
            .expect("rule should be soft deleted");

        let rules = WorkerRule::read_all(&mut conn, account.id).expect("rules should read");

        assert_eq!(rules.len(), 1);
        assert_eq!(rules.first().expect("one rule should remain").id, active.id);
    }

    #[test]
    fn rule_reads_surface_corrupt_row_id() {
        let mut conn = setup_connection();
        let account = test_account();

        diesel::insert_into(rules::table)
            .values(RuleSQLite {
                id: "not-a-uuid".to_string(),
                account_id: account.id.to_string(),
                ..base_sqlite_rule()
            })
            .execute(&mut conn)
            .expect("corrupt rule row should insert for conversion test");

        let error = WorkerRule::read_all(&mut conn, account.id)
            .expect_err("corrupt rule row should fail read conversion");

        assert_error_mentions(error, "id");
    }

    #[test]
    fn rule_worker_reports_missing_table_errors() {
        let mut conn = setup_connection();
        let account = test_account();
        let rule = domain_rule();
        drop_rules_table(&mut conn);

        let error = WorkerRule::create(
            &mut conn,
            &RuleName::RiskPerTrade(2.0),
            "missing table",
            1,
            &RuleLevel::Error,
            &account,
        )
        .expect_err("missing rules table should fail create");
        assert_error_mentions(error, "rules");

        let error = WorkerRule::read_all(&mut conn, account.id)
            .expect_err("missing rules table should fail read all");
        assert_error_mentions(error, "rules");

        let error = WorkerRule::make_inactive(&mut conn, &rule)
            .expect_err("missing rules table should fail make inactive");
        assert_error_mentions(error, "rules");

        let error = WorkerRule::read_for_account_with_name(
            &mut conn,
            account.id,
            &RuleName::RiskPerTrade(2.0),
        )
        .expect_err("missing rules table should fail named read");
        assert_error_mentions(error, "rules");
    }
}
