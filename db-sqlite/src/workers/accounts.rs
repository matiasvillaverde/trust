use crate::error::{ConversionError, IntoDomainModel, IntoDomainModels};
use crate::schema::{accounts, accounts_balances, trades};
use chrono::{NaiveDateTime, Utc};
use diesel::prelude::*;
use model::AccountRead;
use model::{Account, AccountType, AccountWrite, BrokerKind, Environment, Status};
use rust_decimal::Decimal;
use std::error::Error;
use std::str::FromStr;
use std::sync::{Arc, Mutex, MutexGuard};
use tracing::error;
use uuid::Uuid;

/// Database worker for account operations
pub struct AccountDB {
    pub connection: Arc<Mutex<SqliteConnection>>,
}

impl std::fmt::Debug for AccountDB {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AccountDB")
            .field("connection", &"Arc<Mutex<SqliteConnection>>")
            .finish()
    }
}

impl AccountDB {
    fn connection_guard(&self) -> Result<MutexGuard<'_, SqliteConnection>, Box<dyn Error>> {
        self.connection.lock().map_err(|error| {
            format!("failed to acquire account database connection lock: {error}").into()
        })
    }

    fn ensure_no_active_children(
        connection: &mut SqliteConnection,
        account_id: Uuid,
    ) -> Result<(), Box<dyn Error>> {
        let child_names = accounts::table
            .filter(accounts::deleted_at.is_null())
            .filter(accounts::parent_account_id.eq(account_id.to_string()))
            .select(accounts::name)
            .load::<String>(connection)
            .map_err(|error| {
                error!("Error checking child accounts before deletion: {:?}", error);
                error
            })?;

        if child_names.is_empty() {
            return Ok(());
        }

        Err(format!(
            "Cannot delete account with child accounts. Delete child accounts first: {}",
            child_names.join(", ")
        )
        .into())
    }

    fn ensure_no_open_trades(
        connection: &mut SqliteConnection,
        account_id: Uuid,
    ) -> Result<(), Box<dyn Error>> {
        let open_statuses = [
            Status::New,
            Status::Funded,
            Status::Submitted,
            Status::PartiallyFilled,
            Status::Filled,
        ]
        .into_iter()
        .map(|status| status.to_string())
        .collect::<Vec<_>>();

        let open_trade_count = trades::table
            .filter(trades::deleted_at.is_null())
            .filter(trades::account_id.eq(account_id.to_string()))
            .filter(trades::status.eq_any(open_statuses))
            .count()
            .get_result::<i64>(connection)
            .map_err(|error| {
                error!(
                    "Error checking open trades before account deletion: {:?}",
                    error
                );
                error
            })?;

        if open_trade_count == 0 {
            return Ok(());
        }

        Err(
            format!("Cannot delete account {account_id}: {open_trade_count} open trade(s) exist")
                .into(),
        )
    }

    fn ensure_zero_balances(
        connection: &mut SqliteConnection,
        account_id: Uuid,
    ) -> Result<(), Box<dyn Error>> {
        let balance_rows = accounts_balances::table
            .filter(accounts_balances::deleted_at.is_null())
            .filter(accounts_balances::account_id.eq(account_id.to_string()))
            .select((
                accounts_balances::total_balance,
                accounts_balances::total_in_trade,
                accounts_balances::total_available,
                accounts_balances::taxed,
                accounts_balances::total_earnings,
            ))
            .load::<BalanceSafetyRow>(connection)
            .map_err(|error| {
                error!(
                    "Error checking account balances before account deletion: {:?}",
                    error
                );
                error
            })?;

        for row in balance_rows {
            if row.has_non_zero_amount()? {
                return Err(format!(
                    "Cannot delete account {account_id}: account has a non-zero balance. Use --force to bypass the zero-balance check"
                )
                .into());
            }
        }

        Ok(())
    }
}

impl AccountWrite for AccountDB {
    fn create(
        &mut self,
        name: &str,
        description: &str,
        environment: Environment,
        taxes_percentage: Decimal,
        earnings_percentage: Decimal,
    ) -> Result<Account, Box<dyn Error>> {
        self.create_with_profile(
            name,
            description,
            environment,
            taxes_percentage,
            earnings_percentage,
            AccountType::Primary,
            None,
            BrokerKind::Alpaca,
            None,
        )
    }

    fn create_with_hierarchy(
        &mut self,
        name: &str,
        description: &str,
        environment: Environment,
        taxes_percentage: Decimal,
        earnings_percentage: Decimal,
        account_type: AccountType,
        parent_account_id: Option<Uuid>,
    ) -> Result<Account, Box<dyn Error>> {
        self.create_with_profile(
            name,
            description,
            environment,
            taxes_percentage,
            earnings_percentage,
            account_type,
            parent_account_id,
            BrokerKind::Alpaca,
            None,
        )
    }

    fn create_with_profile(
        &mut self,
        name: &str,
        description: &str,
        environment: Environment,
        taxes_percentage: Decimal,
        earnings_percentage: Decimal,
        account_type: AccountType,
        parent_account_id: Option<Uuid>,
        broker_kind: BrokerKind,
        broker_account_id: Option<&str>,
    ) -> Result<Account, Box<dyn Error>> {
        if account_type.requires_parent() && parent_account_id.is_none() {
            return Err("Child account types require a parent account ID".into());
        }

        if account_type == AccountType::Primary && parent_account_id.is_some() {
            return Err("Primary accounts cannot have parent accounts".into());
        }

        let mut guard = self.connection_guard()?;
        let connection: &mut SqliteConnection = &mut guard;

        if let Some(parent_id) = parent_account_id {
            let parent = accounts::table
                .filter(accounts::id.eq(parent_id.to_string()))
                .first::<AccountSQLite>(connection)
                .map_err(|error| {
                    error!("Error reading parent account: {:?}", error);
                    error
                })?;

            if parent.account_type != "primary" {
                return Err("Parent account must be a primary account".into());
            }
        }

        let uuid = Uuid::new_v4().to_string();
        let now = Utc::now().naive_utc();

        let new_account = NewAccount {
            id: uuid,
            created_at: now,
            updated_at: now,
            deleted_at: None,
            name: name.to_lowercase(),
            description: description.to_lowercase(),
            environment: environment.to_string(),
            taxes_percentage: taxes_percentage.to_string(),
            earnings_percentage: earnings_percentage.to_string(),
            account_type: account_type.to_string(),
            parent_account_id: parent_account_id.map(|id| id.to_string()),
            broker_kind: broker_kind.to_string(),
            broker_account_id: broker_account_id.map(ToOwned::to_owned),
        };

        diesel::insert_into(accounts::table)
            .values(&new_account)
            .get_result::<AccountSQLite>(connection)
            .map_err(|error| {
                error!("Error creating hierarchical account: {:?}", error);
                error
            })?
            .into_domain_model()
    }

    fn delete(&mut self, account_id: Uuid, force: bool) -> Result<Account, Box<dyn Error>> {
        let mut guard = self.connection_guard()?;
        let connection: &mut SqliteConnection = &mut guard;

        let account = accounts::table
            .filter(accounts::id.eq(account_id.to_string()))
            .filter(accounts::deleted_at.is_null())
            .first::<AccountSQLite>(connection)
            .map_err(|error| {
                error!("Error reading account before deletion: {:?}", error);
                error
            })?;

        Self::ensure_no_active_children(connection, account_id)?;
        Self::ensure_no_open_trades(connection, account_id)?;
        if !force {
            Self::ensure_zero_balances(connection, account_id)?;
        }

        let now = Utc::now().naive_utc();
        diesel::update(
            accounts_balances::table
                .filter(accounts_balances::account_id.eq(account_id.to_string()))
                .filter(accounts_balances::deleted_at.is_null()),
        )
        .set((
            accounts_balances::updated_at.eq(now),
            accounts_balances::deleted_at.eq(Some(now)),
        ))
        .execute(connection)
        .map_err(|error| {
            error!("Error soft-deleting account balances: {:?}", error);
            error
        })?;

        diesel::update(
            accounts::table
                .filter(accounts::id.eq(account_id.to_string()))
                .filter(accounts::deleted_at.is_null()),
        )
        .set((
            accounts::updated_at.eq(now),
            accounts::deleted_at.eq(Some(now)),
        ))
        .execute(connection)
        .map_err(|error| {
            error!("Error soft-deleting account: {:?}", error);
            error
        })?;

        Ok(Account {
            updated_at: now,
            deleted_at: Some(now),
            ..account.into_domain_model()?
        })
    }
}

impl AccountRead for AccountDB {
    fn for_name(&mut self, name: &str) -> Result<Account, Box<dyn Error>> {
        let mut guard = self.connection_guard()?;
        let connection: &mut SqliteConnection = &mut guard;

        accounts::table
            .filter(accounts::name.eq(name.to_lowercase()))
            .filter(accounts::deleted_at.is_null())
            .first::<AccountSQLite>(connection)
            .map_err(|error| {
                error!("Error reading account: {:?}", error);
                error
            })?
            .into_domain_model()
    }

    fn id(&mut self, id: Uuid) -> Result<Account, Box<dyn Error>> {
        let mut guard = self.connection_guard()?;
        let connection: &mut SqliteConnection = &mut guard;

        accounts::table
            .filter(accounts::id.eq(id.to_string()))
            .filter(accounts::deleted_at.is_null())
            .first::<AccountSQLite>(connection)
            .map_err(|error| {
                error!("Error reading account: {:?}", error);
                error
            })?
            .into_domain_model()
    }

    fn all(&mut self) -> Result<Vec<Account>, Box<dyn Error>> {
        let mut guard = self.connection_guard()?;
        let connection: &mut SqliteConnection = &mut guard;
        accounts::table
            .filter(accounts::deleted_at.is_null())
            .load::<AccountSQLite>(connection)
            .map_err(|error| {
                error!("Error reading all accounts: {:?}", error);
                error
            })?
            .into_domain_models()
    }
}

#[derive(Debug, Queryable, Identifiable, AsChangeset, Insertable)]
#[diesel(table_name = accounts)]
#[diesel(primary_key(id))]
#[diesel(treat_none_as_null = true)]
pub struct AccountSQLite {
    pub id: String,
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
    pub deleted_at: Option<NaiveDateTime>,
    pub name: String,
    pub description: String,
    pub environment: String,
    pub taxes_percentage: String,
    pub earnings_percentage: String,
    pub account_type: String,
    pub parent_account_id: Option<String>,
    pub broker_kind: String,
    pub broker_account_id: Option<String>,
}

impl TryFrom<AccountSQLite> for Account {
    type Error = ConversionError;

    fn try_from(value: AccountSQLite) -> Result<Self, Self::Error> {
        let account_type = match value.account_type.as_str() {
            "primary" => AccountType::Primary,
            "earnings" => AccountType::Earnings,
            "tax_reserve" => AccountType::TaxReserve,
            "reinvestment" => AccountType::Reinvestment,
            "insurance" => AccountType::Insurance,
            _ => {
                return Err(ConversionError::new(
                    "account_type",
                    "Failed to parse account type",
                ))
            }
        };

        let parent_account_id = value
            .parent_account_id
            .as_ref()
            .map(|id| Uuid::parse_str(id))
            .transpose()
            .map_err(|_| {
                ConversionError::new("parent_account_id", "Failed to parse parent account ID")
            })?;

        Ok(Account {
            id: Uuid::parse_str(&value.id)
                .map_err(|_| ConversionError::new("id", "Failed to parse account ID"))?,
            created_at: value.created_at,
            updated_at: value.updated_at,
            deleted_at: value.deleted_at,
            name: value.name,
            description: value.description,
            environment: Environment::from_str(&value.environment)
                .map_err(|_| ConversionError::new("environment", "Failed to parse environment"))?,
            taxes_percentage: Decimal::from_str(&value.taxes_percentage).map_err(|_| {
                ConversionError::new("taxes_percentage", "Failed to parse taxes percentage")
            })?,
            earnings_percentage: Decimal::from_str(&value.earnings_percentage).map_err(|_| {
                ConversionError::new("earnings_percentage", "Failed to parse earnings percentage")
            })?,
            account_type,
            parent_account_id,
            broker_kind: BrokerKind::from_str(&value.broker_kind)
                .map_err(|_| ConversionError::new("broker_kind", "Failed to parse broker kind"))?,
            broker_account_id: value.broker_account_id,
        })
    }
}

impl IntoDomainModel<Account> for AccountSQLite {
    fn into_domain_model(self) -> Result<Account, Box<dyn Error>> {
        self.try_into().map_err(Into::into)
    }
}

#[derive(Insertable)]
#[diesel(table_name = accounts)]
#[diesel(treat_none_as_null = true)]
struct NewAccount {
    id: String,
    created_at: NaiveDateTime,
    updated_at: NaiveDateTime,
    deleted_at: Option<NaiveDateTime>,
    name: String,
    description: String,
    environment: String,
    taxes_percentage: String,
    earnings_percentage: String,
    account_type: String,
    parent_account_id: Option<String>,
    broker_kind: String,
    broker_account_id: Option<String>,
}

#[derive(Queryable)]
struct BalanceSafetyRow {
    total_balance: String,
    total_in_trade: String,
    total_available: String,
    taxed: String,
    total_earnings: String,
}

impl BalanceSafetyRow {
    fn has_non_zero_amount(&self) -> Result<bool, Box<dyn Error>> {
        let amounts = [
            (&self.total_balance, "total_balance"),
            (&self.total_in_trade, "total_in_trade"),
            (&self.total_available, "total_available"),
            (&self.taxed, "taxed"),
            (&self.total_earnings, "total_earnings"),
        ];

        for (value, field) in amounts {
            let amount = Decimal::from_str(value).map_err(|_| {
                ConversionError::new(field, "Failed to parse account balance amount")
            })?;
            if amount != Decimal::ZERO {
                return Ok(true);
            }
        }

        Ok(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workers::{WorkerOrder, WorkerTrade, WorkerTradingVehicle};
    use crate::SqliteDatabase;
    use diesel_migrations::*;
    use model::{
        Currency, DatabaseFactory, DraftTrade, OrderAction, OrderCategory, TradeCategory,
        TradingVehicleCategory,
    };
    use rust_decimal_macros::dec;
    pub const MIGRATIONS: EmbeddedMigrations = embed_migrations!();
    // Declare a test database connection
    fn establish_connection() -> SqliteConnection {
        let mut connection = SqliteConnection::establish(":memory:").unwrap();
        // This will run the necessary migrations.
        connection.run_pending_migrations(MIGRATIONS).unwrap();
        connection.begin_test_transaction().unwrap();
        connection
    }
    fn create_factory(connection: SqliteConnection) -> Box<dyn DatabaseFactory> {
        Box::new(SqliteDatabase::new_from(Arc::new(Mutex::new(connection))))
    }

    fn valid_account_sqlite() -> AccountSQLite {
        let now = Utc::now().naive_utc();
        AccountSQLite {
            id: Uuid::new_v4().to_string(),
            created_at: now,
            updated_at: now,
            deleted_at: None,
            name: "account".to_string(),
            description: "description".to_string(),
            environment: "paper".to_string(),
            taxes_percentage: "20".to_string(),
            earnings_percentage: "80".to_string(),
            account_type: "primary".to_string(),
            parent_account_id: None,
            broker_kind: "alpaca".to_string(),
            broker_account_id: None,
        }
    }

    fn assert_account_conversion_error(row: AccountSQLite, field: &str) {
        let error = Account::try_from(row).unwrap_err();
        assert!(
            error.to_string().contains(&format!("field '{field}'")),
            "unexpected conversion error: {error}"
        );
    }

    fn assert_error_mentions(error: Box<dyn Error>, expected: &str) {
        let message = error.to_string();
        assert!(
            message.contains(expected),
            "expected error to mention {expected:?}, got {message:?}"
        );
    }

    fn account_db() -> AccountDB {
        AccountDB {
            connection: Arc::new(Mutex::new(establish_connection())),
        }
    }

    fn create_account_balance(
        db: &AccountDB,
        account: &Account,
        balance: Decimal,
    ) -> model::AccountBalance {
        let database = SqliteDatabase::new_from(Arc::clone(&db.connection));
        let account_balance = database
            .account_balance_write()
            .create(account, &Currency::USD)
            .expect("account balance should be created");
        database
            .account_balance_write()
            .update(
                &account_balance,
                balance,
                Decimal::ZERO,
                balance,
                Decimal::ZERO,
            )
            .expect("account balance should be updated")
    }

    fn create_open_trade(db: &AccountDB, account: &Account) {
        let mut connection = db
            .connection
            .lock()
            .expect("connection lock should be available");
        let vehicle = WorkerTradingVehicle::create(
            &mut connection,
            "DELETEOPEN",
            Some("DELETEOPEN"),
            &TradingVehicleCategory::Stock,
            "alpaca",
        )
        .expect("trading vehicle should be created");
        let stop = WorkerOrder::create(
            &mut connection,
            dec!(90),
            &Currency::USD,
            dec!(10),
            &OrderAction::Sell,
            &OrderCategory::Stop,
            &vehicle,
        )
        .expect("stop order should be created");
        let entry = WorkerOrder::create(
            &mut connection,
            dec!(100),
            &Currency::USD,
            dec!(10),
            &OrderAction::Buy,
            &OrderCategory::Limit,
            &vehicle,
        )
        .expect("entry order should be created");
        let target = WorkerOrder::create(
            &mut connection,
            dec!(120),
            &Currency::USD,
            dec!(10),
            &OrderAction::Sell,
            &OrderCategory::Limit,
            &vehicle,
        )
        .expect("target order should be created");

        WorkerTrade::create(
            &mut connection,
            DraftTrade {
                account: account.clone(),
                trading_vehicle: vehicle,
                quantity: 10.into(),
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
        .expect("open trade should be created");
    }

    fn poisoned_account_db() -> AccountDB {
        let connection = Arc::new(Mutex::new(establish_connection()));
        let poisoned_connection = Arc::clone(&connection);
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _guard = poisoned_connection
                .lock()
                .expect("connection lock should be acquired before poisoning");
            std::panic::resume_unwind(Box::new("poison account connection lock"));
        }));
        AccountDB { connection }
    }

    fn assert_connection_lock_error<T>(result: Result<T, Box<dyn Error>>) {
        assert!(result.is_err());
        let error = result.err().expect("operation should fail");
        assert!(error
            .to_string()
            .contains("failed to acquire account database connection lock"));
    }

    fn drop_accounts_table(db: &mut AccountDB) {
        let mut conn = db
            .connection
            .lock()
            .expect("connection lock should be available");
        diesel::sql_query("DROP TABLE accounts")
            .execute(&mut *conn)
            .expect("accounts table should be dropped");
    }

    #[test]
    fn debug_representation_hides_connection_internals() {
        let conn = establish_connection();
        let db = AccountDB {
            connection: Arc::new(Mutex::new(conn)),
        };

        assert_eq!(
            format!("{db:?}"),
            "AccountDB { connection: \"Arc<Mutex<SqliteConnection>>\" }"
        );
    }

    #[test]
    fn account_methods_return_errors_when_connection_lock_is_poisoned() {
        let mut db = poisoned_account_db();

        assert_connection_lock_error(db.create(
            "Locked Account",
            "locked",
            Environment::Paper,
            dec!(20),
            dec!(80),
        ));
        assert_connection_lock_error(db.delete(Uuid::new_v4(), false));
        assert_connection_lock_error(db.for_name("locked account"));
        assert_connection_lock_error(db.id(Uuid::new_v4()));
        assert_connection_lock_error(db.all());
    }

    #[test]
    fn test_create_account() {
        let conn: SqliteConnection = establish_connection();
        let mut db = AccountDB {
            connection: Arc::new(Mutex::new(conn)),
        };
        // Create a new account record
        let account = db
            .create(
                "Test Account",
                "This is a test account",
                Environment::Paper,
                dec!(20),
                dec!(80),
            )
            .expect("Error creating account");
        assert_eq!(account.name, "test account"); // it should be lowercase
        assert_eq!(account.description, "this is a test account"); // it should be lowercase
        assert_eq!(account.environment, Environment::Paper);
        assert_eq!(account.deleted_at, None);
    }

    #[test]
    fn account_sqlite_conversion_reports_corrupt_fields() {
        let mut invalid_id = valid_account_sqlite();
        invalid_id.id = "not-a-uuid".to_string();
        assert_account_conversion_error(invalid_id, "id");

        let mut invalid_environment = valid_account_sqlite();
        invalid_environment.environment = "sandbox".to_string();
        assert_account_conversion_error(invalid_environment, "environment");

        let mut invalid_taxes = valid_account_sqlite();
        invalid_taxes.taxes_percentage = "nan".to_string();
        assert_account_conversion_error(invalid_taxes, "taxes_percentage");

        let mut invalid_earnings = valid_account_sqlite();
        invalid_earnings.earnings_percentage = "nan".to_string();
        assert_account_conversion_error(invalid_earnings, "earnings_percentage");

        let mut invalid_account_type = valid_account_sqlite();
        invalid_account_type.account_type = "settlement".to_string();
        assert_account_conversion_error(invalid_account_type, "account_type");

        let mut invalid_parent = valid_account_sqlite();
        invalid_parent.parent_account_id = Some("not-a-uuid".to_string());
        assert_account_conversion_error(invalid_parent, "parent_account_id");

        let mut invalid_broker = valid_account_sqlite();
        invalid_broker.broker_kind = "paper-broker".to_string();
        assert_account_conversion_error(invalid_broker, "broker_kind");
    }

    #[test]
    fn create_with_profile_enforces_hierarchy_and_preserves_broker_profile() {
        let conn = establish_connection();
        let mut db = AccountDB {
            connection: Arc::new(Mutex::new(conn)),
        };

        let missing_parent = db
            .create_with_hierarchy(
                "Earnings without parent",
                "child account",
                Environment::Paper,
                dec!(20),
                dec!(80),
                AccountType::Earnings,
                None,
            )
            .unwrap_err();
        assert!(missing_parent
            .to_string()
            .contains("Child account types require a parent account ID"));

        let primary_with_parent = db
            .create_with_hierarchy(
                "Primary with parent",
                "invalid parent",
                Environment::Paper,
                dec!(20),
                dec!(80),
                AccountType::Primary,
                Some(Uuid::new_v4()),
            )
            .unwrap_err();
        assert!(primary_with_parent
            .to_string()
            .contains("Primary accounts cannot have parent accounts"));

        let parent = db
            .create_with_profile(
                "Parent Account",
                "primary",
                Environment::Paper,
                dec!(20),
                dec!(80),
                AccountType::Primary,
                None,
                BrokerKind::Ibkr,
                Some("DU12345"),
            )
            .expect("primary account with broker profile should persist");
        assert_eq!(parent.broker_kind, BrokerKind::Ibkr);
        assert_eq!(parent.broker_account_id.as_deref(), Some("DU12345"));

        let child = db
            .create_with_hierarchy(
                "Earnings Account",
                "child",
                Environment::Paper,
                dec!(20),
                dec!(80),
                AccountType::Earnings,
                Some(parent.id),
            )
            .expect("child account should persist under a primary parent");
        assert_eq!(child.account_type, AccountType::Earnings);
        assert_eq!(child.parent_account_id, Some(parent.id));

        let non_primary_parent = db
            .create_with_hierarchy(
                "Tax Reserve",
                "invalid child",
                Environment::Paper,
                dec!(20),
                dec!(80),
                AccountType::TaxReserve,
                Some(child.id),
            )
            .unwrap_err();
        assert!(non_primary_parent
            .to_string()
            .contains("Parent account must be a primary account"));
    }

    #[test]
    fn create_with_hierarchy_reports_missing_parent_lookup_errors() {
        let mut db = account_db();

        let error = db
            .create_with_hierarchy(
                "Earnings Account",
                "missing parent",
                Environment::Paper,
                dec!(20),
                dec!(80),
                AccountType::Earnings,
                Some(Uuid::new_v4()),
            )
            .expect_err("missing parent account should fail child creation");

        assert_error_mentions(error, "not found");
    }

    #[test]
    fn test_read_account() {
        let conn = establish_connection();
        let mut db = AccountDB {
            connection: Arc::new(Mutex::new(conn)),
        };
        // Create a new account record
        let created_account = db
            .create(
                "Test Account",
                "This is a test account",
                Environment::Paper,
                dec!(20),
                dec!(80),
            )
            .expect("Error creating account");
        // Read the account record by name
        let read_account = db
            .for_name("Test Account")
            .expect("Account should be found");
        assert_eq!(read_account, created_account);
    }
    #[test]
    fn test_read_account_id() {
        let conn = establish_connection();
        let mut db = AccountDB {
            connection: Arc::new(Mutex::new(conn)),
        };
        // Create a new account record
        let created_account = db
            .create(
                "Test Account",
                "This is a test account",
                Environment::Paper,
                dec!(20),
                dec!(80),
            )
            .expect("Error creating account");
        // Read the account record by name
        let read_account = db.id(created_account.id).expect("Account should be found");
        assert_eq!(read_account, created_account);
    }
    #[test]
    fn test_create_account_same_name() {
        let conn = establish_connection();
        let mut db = AccountDB {
            connection: Arc::new(Mutex::new(conn)),
        };
        let name = "Test Account";
        // Create a new account record
        db.create(
            name,
            "This is a test account",
            Environment::Paper,
            dec!(20),
            dec!(80),
        )
        .expect("Error creating account");
        // Create a new account record with the same name
        db.create(
            name,
            "This is a test account",
            Environment::Paper,
            dec!(20),
            dec!(80),
        )
        .expect_err("Error creating account with same name");
    }
    #[test]
    fn test_read_account_not_found() {
        let conn = establish_connection();
        let mut db = AccountDB {
            connection: Arc::new(Mutex::new(conn)),
        };
        db.for_name("Non existent account")
            .expect_err("Account should not be found");
    }
    #[test]
    fn test_read_all_accounts() {
        let db = create_factory(establish_connection());
        let created_accounts = vec![
            db.account_write()
                .create(
                    "Test Account 1",
                    "This is a test account",
                    Environment::Paper,
                    dec!(20),
                    dec!(80),
                )
                .expect("Error creating account"),
            db.account_write()
                .create(
                    "Test Account 2",
                    "This is a test account",
                    Environment::Paper,
                    dec!(20),
                    dec!(80),
                )
                .expect("Error creating account"),
            db.account_write()
                .create(
                    "Test Account 3",
                    "This is a test account",
                    Environment::Paper,
                    dec!(20),
                    dec!(80),
                )
                .expect("Error creating account"),
        ];

        // Read all account records
        let accounts = db.account_read().all().expect("Error reading all accounts");
        assert_eq!(accounts, created_accounts);
    }

    #[test]
    fn read_all_filters_soft_deleted_accounts() {
        let mut db = account_db();
        let active = db
            .create(
                "Active Account",
                "active",
                Environment::Paper,
                dec!(20),
                dec!(80),
            )
            .expect("active account should be created");
        let deleted = db
            .create(
                "Deleted Account",
                "deleted",
                Environment::Paper,
                dec!(20),
                dec!(80),
            )
            .expect("deleted account should be created");

        {
            let mut conn = db
                .connection
                .lock()
                .expect("connection lock should be available");
            diesel::update(accounts::table.filter(accounts::id.eq(deleted.id.to_string())))
                .set(accounts::deleted_at.eq(Some(Utc::now().naive_utc())))
                .execute(&mut *conn)
                .expect("account should be soft deleted");
        }

        let accounts = db.all().expect("accounts should read");

        assert_eq!(accounts.len(), 1);
        assert_eq!(accounts.first().expect("one active account").id, active.id);
    }

    #[test]
    fn delete_soft_deletes_zero_balance_account_and_hides_it_from_reads() {
        let mut db = account_db();
        let account = db
            .create(
                "Delete Zero Balance",
                "delete",
                Environment::Paper,
                dec!(20),
                dec!(80),
            )
            .expect("account should be created");
        create_account_balance(&db, &account, Decimal::ZERO);

        let deleted = db
            .delete(account.id, false)
            .expect("zero balance account should delete");

        assert_eq!(deleted.id, account.id);
        assert!(deleted.deleted_at.is_some());
        assert!(db.id(account.id).is_err());
        assert!(db.for_name("delete zero balance").is_err());
        assert!(db.all().expect("active accounts should read").is_empty());

        let database = SqliteDatabase::new_from(Arc::clone(&db.connection));
        assert!(database
            .account_balance_read()
            .for_account(account.id)
            .expect("balances should read")
            .is_empty());
    }

    #[test]
    fn delete_rejects_non_zero_balance_unless_forced() {
        let mut db = account_db();
        let account = db
            .create(
                "Delete Non Zero Balance",
                "delete",
                Environment::Paper,
                dec!(20),
                dec!(80),
            )
            .expect("account should be created");
        create_account_balance(&db, &account, dec!(25));

        let error = db
            .delete(account.id, false)
            .expect_err("non-zero account should require force");
        assert_error_mentions(error, "non-zero balance");

        let deleted = db
            .delete(account.id, true)
            .expect("force should bypass zero-balance check");
        assert_eq!(deleted.id, account.id);
        assert!(deleted.deleted_at.is_some());
    }

    #[test]
    fn delete_rejects_accounts_with_active_children() {
        let mut db = account_db();
        let parent = db
            .create(
                "Delete Parent",
                "delete",
                Environment::Paper,
                dec!(20),
                dec!(80),
            )
            .expect("parent account should be created");
        let child = db
            .create_with_hierarchy(
                "Delete Child",
                "child",
                Environment::Paper,
                dec!(20),
                dec!(80),
                AccountType::Earnings,
                Some(parent.id),
            )
            .expect("child account should be created");

        let error = db
            .delete(parent.id, true)
            .expect_err("parent with active child should not delete");
        assert_error_mentions(error, "child accounts");

        db.delete(child.id, true)
            .expect("child account should delete first");
        db.delete(parent.id, true)
            .expect("parent should delete after child");
    }

    #[test]
    fn delete_rejects_accounts_with_open_trades() {
        let mut db = account_db();
        let account = db
            .create(
                "Delete Open Trade",
                "delete",
                Environment::Paper,
                dec!(20),
                dec!(80),
            )
            .expect("account should be created");
        create_open_trade(&db, &account);

        let error = db
            .delete(account.id, true)
            .expect_err("open trade account should not delete");
        assert_error_mentions(error, "open trade");
    }

    #[test]
    fn read_all_surfaces_corrupt_row_id() {
        let mut db = account_db();
        {
            let mut conn = db
                .connection
                .lock()
                .expect("connection lock should be available");
            diesel::insert_into(accounts::table)
                .values(AccountSQLite {
                    id: "not-a-uuid".to_string(),
                    ..valid_account_sqlite()
                })
                .execute(&mut *conn)
                .expect("corrupt account row should insert for conversion test");
        }

        let error = db
            .all()
            .expect_err("corrupt account row should fail conversion");

        assert_error_mentions(error, "id");
    }

    #[test]
    fn account_worker_reports_missing_table_errors() {
        let mut db = account_db();
        drop_accounts_table(&mut db);

        let error = db
            .create(
                "Missing Table Account",
                "create",
                Environment::Paper,
                dec!(20),
                dec!(80),
            )
            .expect_err("missing accounts table should fail create");
        assert_error_mentions(error, "accounts");

        let error = db
            .delete(Uuid::new_v4(), true)
            .expect_err("missing accounts table should fail delete");
        assert_error_mentions(error, "accounts");

        let error = db
            .create_with_hierarchy(
                "Child Account",
                "parent read",
                Environment::Paper,
                dec!(20),
                dec!(80),
                AccountType::Earnings,
                Some(Uuid::new_v4()),
            )
            .expect_err("missing accounts table should fail parent lookup");
        assert_error_mentions(error, "accounts");

        let error = db
            .for_name("missing")
            .expect_err("missing accounts table should fail name read");
        assert_error_mentions(error, "accounts");

        let error = db
            .id(Uuid::new_v4())
            .expect_err("missing accounts table should fail id read");
        assert_error_mentions(error, "accounts");

        let error = db
            .all()
            .expect_err("missing accounts table should fail all read");
        assert_error_mentions(error, "accounts");
    }
}
