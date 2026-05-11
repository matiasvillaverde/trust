use crate::error::{ConversionError, IntoDomainModel, IntoDomainModels};
use crate::schema::transactions;
use chrono::{Datelike, NaiveDate, NaiveDateTime, NaiveTime, Utc};
use diesel::prelude::*;
use model::{Currency, Status, Transaction, TransactionCategory};
use rust_decimal::Decimal;
use std::error::Error;
use std::str::FromStr;
use tracing::error;
use uuid::Uuid;

use super::WorkerTrade;

/// Worker for handling transaction database operations
#[derive(Debug)]
pub struct WorkerTransaction;

impl WorkerTransaction {
    pub fn create_transaction(
        connection: &mut SqliteConnection,
        account_id: Uuid,
        amount: Decimal,
        currency: &Currency,
        category: TransactionCategory,
    ) -> Result<Transaction, Box<dyn Error>> {
        Self::create_transaction_with_id(
            connection,
            Uuid::new_v4(),
            account_id,
            amount,
            currency,
            category,
        )
    }

    pub fn create_transaction_with_id(
        connection: &mut SqliteConnection,
        transaction_id: Uuid,
        account_id: Uuid,
        amount: Decimal,
        currency: &Currency,
        category: TransactionCategory,
    ) -> Result<Transaction, Box<dyn Error>> {
        let now = Utc::now().naive_utc();

        let new_transaction = NewTransaction {
            id: transaction_id.to_string(),
            created_at: now,
            updated_at: now,
            deleted_at: None,
            currency: currency.to_string(),
            category: category.to_string(),
            account_id: account_id.to_string(),
            amount: amount.to_string(),
            trade_id: category.trade_id().map(|uuid| uuid.to_string()),
        };

        diesel::insert_into(transactions::table)
            .values(&new_transaction)
            .execute(connection)
            .map_err(|error| {
                error!("Error creating transaction: {:?}", error);
                error
            })?;

        Ok(Transaction {
            id: Uuid::parse_str(&new_transaction.id)?,
            created_at: new_transaction.created_at,
            updated_at: new_transaction.updated_at,
            deleted_at: new_transaction.deleted_at,
            category,
            currency: *currency,
            amount,
            account_id,
        })
    }

    pub fn read_all_transactions(
        connection: &mut SqliteConnection,
        account_id: Uuid,
        currency: &Currency,
    ) -> Result<Vec<Transaction>, Box<dyn Error>> {
        let transactions = transactions::table
            .filter(transactions::deleted_at.is_null())
            .filter(transactions::account_id.eq(account_id.to_string()))
            .filter(transactions::currency.eq(currency.to_string()))
            .load::<TransactionSQLite>(connection)
            .map_err(|error| {
                error!("Error reading all transactions: {:?}", error);
                error
            })?
            .into_domain_models()?;
        Ok(transactions)
    }

    pub fn read_all_trade_transactions_excluding_taxes(
        connection: &mut SqliteConnection,
        account_id: Uuid,
        currency: &Currency,
    ) -> Result<Vec<Transaction>, Box<dyn Error>> {
        // Hot path: avoid N queries per category; fetch via a single `IN (...)` query.
        // This intentionally excludes tax reserve flows; use `read_all_account_transactions_taxes`
        // for tax-specific reporting.
        // Use string literals to avoid borrowing from temporary enum values.
        let included_categories: [&str; 8] = [
            "deposit",
            "withdrawal",
            "withdrawal_earnings",
            "fee_open",
            "fee_close",
            "fund_trade",
            "payment_from_trade",
            "payment_earnings",
        ];

        let transactions = transactions::table
            .filter(transactions::deleted_at.is_null())
            .filter(transactions::account_id.eq(account_id.to_string()))
            .filter(transactions::currency.eq(currency.to_string()))
            .filter(transactions::category.eq_any(included_categories))
            .order((transactions::created_at.asc(), transactions::id.asc()))
            .load::<TransactionSQLite>(connection)
            .map_err(|error| {
                error!(
                    "Error reading all transactions excluding taxes: {:?}",
                    error
                );
                error
            })?
            .into_domain_models()?;

        Ok(transactions)
    }

    pub fn all_account_transactions_in_trade(
        connection: &mut SqliteConnection,
        account_id: Uuid,
        currency: &Currency,
    ) -> Result<Vec<Transaction>, Box<dyn Error>> {
        // Here we are getting all the transactions for a given account and currency
        // and then filtering them in memory to only include transactions that are
        // part of a trade that is either Funded, Submitted, or Filled.
        // All this transactions are part of a trade that is using the money
        // Either in the market or in the process of being filled or submitted.
        let funded_trades = WorkerTrade::read_all_trades_with_status_currency(
            connection,
            account_id,
            Status::Funded,
            currency,
        )?;

        let funded_tx: Vec<Transaction> = funded_trades
            .into_iter()
            .flat_map(|trade| {
                WorkerTransaction::read_all_trade_transactions_for_category(
                    connection,
                    trade.id,
                    TransactionCategory::FundTrade(Uuid::new_v4()),
                )
            })
            .flatten()
            .collect();

        let submitted_trades = WorkerTrade::read_all_trades_with_status_currency(
            connection,
            account_id,
            Status::Submitted,
            currency,
        )?;

        let filled_trades = WorkerTrade::read_all_trades_with_status_currency(
            connection,
            account_id,
            Status::Filled,
            currency,
        )?;

        let in_market_trades = submitted_trades.into_iter().chain(filled_trades);

        let submitted_trades: Vec<Transaction> = in_market_trades
            .into_iter()
            .flat_map(|trade| {
                WorkerTransaction::read_all_trade_transactions_for_category(
                    connection,
                    trade.id,
                    TransactionCategory::OpenTrade(Uuid::new_v4()),
                )
            })
            .flatten()
            .collect();

        Ok(funded_tx.into_iter().chain(submitted_trades).collect())
    }

    pub fn read_all_account_transactions_taxes(
        connection: &mut SqliteConnection,
        account_id: Uuid,
        currency: &Currency,
    ) -> Result<Vec<Transaction>, Box<dyn Error>> {
        let tx_payments_tax = WorkerTransaction::read_all_account_transactions_for_category(
            connection,
            account_id,
            currency,
            TransactionCategory::PaymentTax(Uuid::new_v4()),
        )?;
        let tx_withdrawal_tax = WorkerTransaction::read_all_account_transactions_for_category(
            connection,
            account_id,
            currency,
            TransactionCategory::WithdrawalTax,
        )?;

        Ok(tx_payments_tax
            .into_iter()
            .chain(tx_withdrawal_tax)
            .collect())
    }

    pub fn read_all_account_transactions_for_category(
        connection: &mut SqliteConnection,
        account_id: Uuid,
        currency: &Currency,
        category: TransactionCategory,
    ) -> Result<Vec<Transaction>, Box<dyn Error>> {
        let transactions = transactions::table
            .filter(transactions::deleted_at.is_null())
            .filter(transactions::account_id.eq(account_id.to_string()))
            .filter(transactions::currency.eq(currency.to_string()))
            .filter(transactions::category.eq(category.key()))
            .order((transactions::created_at.asc(), transactions::id.asc()))
            .load::<TransactionSQLite>(connection)
            .map_err(|error| {
                error!("Error reading transactions: {:?}", error);
                error
            })?
            .into_domain_models()?;
        Ok(transactions)
    }

    pub fn read_all_trade_transactions_for_category(
        connection: &mut SqliteConnection,
        trade_id: Uuid,
        category: TransactionCategory,
    ) -> Result<Vec<Transaction>, Box<dyn Error>> {
        let transactions = transactions::table
            .filter(transactions::deleted_at.is_null())
            .filter(transactions::trade_id.eq(trade_id.to_string()))
            .filter(transactions::category.eq(category.key()))
            .order((transactions::created_at.asc(), transactions::id.asc()))
            .load::<TransactionSQLite>(connection)
            .map_err(|error| {
                error!("Error creating price: {:?}", error);
                error
            })?
            .into_domain_models()?;
        Ok(transactions)
    }

    pub fn read_all_trade_transactions(
        connection: &mut SqliteConnection,
        trade: Uuid,
    ) -> Result<Vec<Transaction>, Box<dyn Error>> {
        let transactions = transactions::table
            .filter(transactions::deleted_at.is_null())
            .filter(transactions::trade_id.eq(trade.to_string()))
            .order((transactions::created_at.asc(), transactions::id.asc()))
            .load::<TransactionSQLite>(connection)
            .map_err(|error| {
                error!("Error reading trade transactions: {:?}", error);
                error
            })?
            .into_domain_models()?;
        Ok(transactions)
    }

    pub fn read_all_transaction_excluding_current_month_and_taxes(
        connection: &mut SqliteConnection,
        account_id: Uuid,
        currency: &Currency,
    ) -> Result<Vec<Transaction>, Box<dyn Error>> {
        let now = Utc::now().naive_utc();
        let first_day_of_month =
            NaiveDate::from_ymd_opt(now.year(), now.month(), 1).ok_or("Failed to create date")?;
        let first_day_of_month = NaiveDateTime::new(
            first_day_of_month,
            NaiveTime::from_hms_opt(0, 0, 0).ok_or("Failed to create time")?,
        );

        // Keep this aligned with the calculator(s) that consume it.
        let included_categories: [&str; 8] = [
            "deposit",
            "withdrawal",
            "withdrawal_earnings",
            "fee_open",
            "fee_close",
            "fund_trade",
            "payment_from_trade",
            "payment_earnings",
        ];

        let tx = transactions::table
            .filter(transactions::deleted_at.is_null())
            .filter(transactions::account_id.eq(account_id.to_string()))
            .filter(transactions::created_at.le(first_day_of_month))
            .filter(transactions::currency.eq(currency.to_string()))
            .filter(transactions::category.eq_any(included_categories))
            .order((transactions::created_at.asc(), transactions::id.asc()))
            .load::<TransactionSQLite>(connection)
            .map_err(|error| {
                error!(
                    "Error reading beginning-of-month transactions excluding taxes: {:?}",
                    error
                );
                error
            })?
            .into_domain_models()?;

        Ok(tx)
    }

    // NOTE: historically we fetched these by category (N queries). The optimized codepaths
    // above use single `IN (...)` queries instead to keep perf stable.
}

#[derive(Debug, Queryable, Identifiable, AsChangeset, Insertable)]
#[diesel(table_name = transactions)]
pub struct TransactionSQLite {
    pub id: String,
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
    pub deleted_at: Option<NaiveDateTime>,
    pub currency: String,
    pub category: String,
    pub amount: String,
    pub account_id: String,
    pub trade_id: Option<String>,
}

impl TryFrom<TransactionSQLite> for Transaction {
    type Error = ConversionError;

    fn try_from(value: TransactionSQLite) -> Result<Self, Self::Error> {
        let trade_id = value
            .trade_id
            .clone()
            .and_then(|uuid| Uuid::parse_str(&uuid).ok());

        let category = TransactionCategory::parse(&value.category, trade_id).map_err(|_| {
            ConversionError::new("category", "Failed to parse transaction category")
        })?;

        Ok(Transaction {
            id: Uuid::parse_str(&value.id)
                .map_err(|_| ConversionError::new("id", "Failed to parse transaction ID"))?,
            created_at: value.created_at,
            updated_at: value.updated_at,
            deleted_at: value.deleted_at,
            category,
            currency: Currency::from_str(&value.currency)
                .map_err(|_| ConversionError::new("currency", "Failed to parse currency"))?,
            amount: Decimal::from_str(&value.amount)
                .map_err(|_| ConversionError::new("amount", "Failed to parse amount"))?,
            account_id: Uuid::parse_str(&value.account_id)
                .map_err(|_| ConversionError::new("account_id", "Failed to parse account ID"))?,
        })
    }
}

impl IntoDomainModel<Transaction> for TransactionSQLite {
    fn into_domain_model(self) -> Result<Transaction, Box<dyn Error>> {
        self.try_into().map_err(Into::into)
    }
}

#[derive(Debug, Insertable)]
#[diesel(table_name = transactions)]
#[diesel(treat_none_as_null = true)]
pub struct NewTransaction {
    pub id: String,
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
    pub deleted_at: Option<NaiveDateTime>,
    pub currency: String,
    pub category: String,
    pub amount: String,
    pub account_id: String,
    pub trade_id: Option<String>,
}

#[cfg(test)]
mod tests {
    use chrono::Duration;
    use diesel::insert_into;
    use rust_decimal_macros::dec;

    use super::*;
    use crate::SqliteDatabase;
    use diesel_migrations::*;
    use model::{
        Account, DatabaseFactory, DraftTrade, Environment, OrderAction, OrderCategory,
        TradeCategory, TradingVehicleCategory,
    };
    use std::sync::{Arc, Mutex};

    pub const MIGRATIONS: EmbeddedMigrations = embed_migrations!();

    // Declare a test database connection
    fn establish_connection() -> SqliteConnection {
        let mut connection = SqliteConnection::establish(":memory:").unwrap();
        // This will run the necessary migrations.
        connection.run_pending_migrations(MIGRATIONS).unwrap();
        connection.begin_test_transaction().unwrap();
        connection
    }

    fn create_factory() -> Box<dyn DatabaseFactory> {
        Box::new(SqliteDatabase::new_from(Arc::new(Mutex::new(
            establish_connection(),
        ))))
    }

    fn create_database_with_connection() -> (SqliteDatabase, Arc<Mutex<SqliteConnection>>) {
        let connection = Arc::new(Mutex::new(establish_connection()));
        (SqliteDatabase::new_from(connection.clone()), connection)
    }

    fn create_account(database: &SqliteDatabase, name: &str) -> Account {
        database
            .account_write()
            .create(name, name, Environment::Paper, dec!(0.0), dec!(0.0))
            .expect("account should be created")
    }

    fn create_persisted_trade(
        database: &SqliteDatabase,
        account: &Account,
        symbol: &str,
    ) -> model::Trade {
        let vehicle = database
            .trading_vehicle_write()
            .create_trading_vehicle(
                symbol,
                Some(symbol),
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
            thesis: None,
            sector: None,
            asset_class: None,
            context: None,
        };

        database
            .trade_write()
            .create_trade(draft, &stop, &entry, &target)
            .expect("trade should be created")
    }

    fn first_day_of_current_month() -> NaiveDateTime {
        let now = Utc::now().naive_utc();
        let first_day =
            NaiveDate::from_ymd_opt(now.year(), now.month(), 1).expect("valid first day");
        NaiveDateTime::new(
            first_day,
            NaiveTime::from_hms_opt(0, 0, 0).expect("valid midnight"),
        )
    }

    fn insert_transaction_row(
        connection: &mut SqliteConnection,
        account_id: Uuid,
        category: TransactionCategory,
        currency: Currency,
        amount: Decimal,
        created_at: NaiveDateTime,
        deleted_at: Option<NaiveDateTime>,
    ) -> Uuid {
        let id = Uuid::new_v4();
        let new_transaction = NewTransaction {
            id: id.to_string(),
            created_at,
            updated_at: created_at,
            deleted_at,
            currency: currency.to_string(),
            category: category.to_string(),
            amount: amount.to_string(),
            account_id: account_id.to_string(),
            trade_id: category.trade_id().map(|trade_id| trade_id.to_string()),
        };

        insert_into(transactions::table)
            .values(&new_transaction)
            .execute(connection)
            .expect("transaction row should be inserted");
        id
    }

    fn transaction_ids(transactions: &[Transaction]) -> Vec<Uuid> {
        transactions
            .iter()
            .map(|transaction| transaction.id)
            .collect()
    }

    fn set_trade_status(connection: &mut SqliteConnection, trade_id: Uuid, status: Status) {
        diesel::update(
            crate::schema::trades::table.filter(crate::schema::trades::id.eq(trade_id.to_string())),
        )
        .set(crate::schema::trades::status.eq(status.to_string()))
        .execute(connection)
        .expect("trade status should be updated");
    }

    fn soft_delete_trading_vehicle(connection: &mut SqliteConnection, trading_vehicle_id: Uuid) {
        diesel::update(
            crate::schema::trading_vehicles::table
                .filter(crate::schema::trading_vehicles::id.eq(trading_vehicle_id.to_string())),
        )
        .set(crate::schema::trading_vehicles::deleted_at.eq(Some(Utc::now().naive_utc())))
        .execute(connection)
        .expect("trading vehicle should be soft deleted");
    }

    fn transaction_row() -> TransactionSQLite {
        let now = Utc::now().naive_utc();
        TransactionSQLite {
            id: Uuid::new_v4().to_string(),
            created_at: now,
            updated_at: now,
            deleted_at: None,
            currency: Currency::USD.to_string(),
            category: TransactionCategory::Deposit.to_string(),
            amount: dec!(100).to_string(),
            account_id: Uuid::new_v4().to_string(),
            trade_id: None,
        }
    }

    fn assert_transaction_conversion_error(row: TransactionSQLite, field: &str) {
        let err = Transaction::try_from(row).expect_err("conversion should fail");
        assert!(err.to_string().contains(field));
    }

    #[derive(Debug)]
    struct AccountQueryFixtureIds {
        previous_deposit_id: Uuid,
        current_withdrawal_id: Uuid,
        earnings_id: Uuid,
    }

    fn insert_core_account_query_rows(
        connection: &mut SqliteConnection,
        account_id: Uuid,
        previous_day: NaiveDateTime,
    ) -> AccountQueryFixtureIds {
        let previous_deposit_id = insert_transaction_row(
            connection,
            account_id,
            TransactionCategory::Deposit,
            Currency::USD,
            dec!(100),
            previous_day,
            None,
        );
        insert_transaction_row(
            connection,
            account_id,
            TransactionCategory::WithdrawalTax,
            Currency::USD,
            dec!(25),
            previous_day,
            None,
        );
        let current_withdrawal_id = insert_transaction_row(
            connection,
            account_id,
            TransactionCategory::Withdrawal,
            Currency::USD,
            dec!(-10),
            Utc::now().naive_utc(),
            None,
        );
        let earnings_id = insert_transaction_row(
            connection,
            account_id,
            TransactionCategory::WithdrawalEarnings,
            Currency::USD,
            dec!(-5),
            previous_day,
            None,
        );

        AccountQueryFixtureIds {
            previous_deposit_id,
            current_withdrawal_id,
            earnings_id,
        }
    }

    fn insert_account_query_noise_rows(
        connection: &mut SqliteConnection,
        account_id: Uuid,
        other_account_id: Uuid,
        previous_day: NaiveDateTime,
    ) {
        let deleted_at = Utc::now().naive_utc();
        insert_transaction_row(
            connection,
            account_id,
            TransactionCategory::Deposit,
            Currency::EUR,
            dec!(100),
            previous_day,
            None,
        );
        insert_transaction_row(
            connection,
            other_account_id,
            TransactionCategory::Deposit,
            Currency::USD,
            dec!(100),
            previous_day,
            None,
        );
        insert_transaction_row(
            connection,
            account_id,
            TransactionCategory::Deposit,
            Currency::USD,
            dec!(999),
            previous_day,
            Some(deleted_at),
        );
    }

    fn assert_active_account_transactions(
        connection: &mut SqliteConnection,
        account_id: Uuid,
        fixture_ids: &AccountQueryFixtureIds,
    ) {
        let all_active =
            WorkerTransaction::read_all_transactions(connection, account_id, &Currency::USD)
                .expect("active account transactions should be readable");
        let all_active_ids = transaction_ids(&all_active);
        assert_eq!(all_active.len(), 4);
        assert!(all_active_ids.contains(&fixture_ids.previous_deposit_id));
        assert!(all_active_ids.contains(&fixture_ids.current_withdrawal_id));
    }

    fn assert_non_tax_account_transactions(
        connection: &mut SqliteConnection,
        account_id: Uuid,
        fixture_ids: &AccountQueryFixtureIds,
    ) {
        let non_tax = WorkerTransaction::read_all_trade_transactions_excluding_taxes(
            connection,
            account_id,
            &Currency::USD,
        )
        .expect("non-tax account transactions should be readable");
        let non_tax_ids = transaction_ids(&non_tax);
        assert_eq!(non_tax.len(), 3);
        assert!(non_tax_ids.contains(&fixture_ids.earnings_id));
        assert!(!non_tax
            .iter()
            .any(|transaction| transaction.category == TransactionCategory::WithdrawalTax));
    }

    fn assert_tax_account_transactions(connection: &mut SqliteConnection, account_id: Uuid) {
        let tax_transactions = WorkerTransaction::read_all_account_transactions_taxes(
            connection,
            account_id,
            &Currency::USD,
        )
        .expect("tax transactions should be readable");
        assert_eq!(tax_transactions.len(), 1);
        assert_eq!(
            tax_transactions
                .first()
                .expect("tax transaction should exist")
                .category,
            TransactionCategory::WithdrawalTax
        );
    }

    fn assert_prior_month_non_tax_transactions(
        connection: &mut SqliteConnection,
        account_id: Uuid,
        fixture_ids: &AccountQueryFixtureIds,
    ) {
        let prior_month =
            WorkerTransaction::read_all_transaction_excluding_current_month_and_taxes(
                connection,
                account_id,
                &Currency::USD,
            )
            .expect("prior month non-tax transactions should be readable");
        let prior_month_ids = transaction_ids(&prior_month);
        assert_eq!(prior_month.len(), 2);
        assert!(prior_month_ids.contains(&fixture_ids.previous_deposit_id));
        assert!(prior_month_ids.contains(&fixture_ids.earnings_id));
        assert!(!prior_month_ids.contains(&fixture_ids.current_withdrawal_id));
    }

    #[test]
    fn test_create_transaction() {
        let db: Box<dyn DatabaseFactory> = create_factory();

        // Create a new account record
        let account = db
            .account_write()
            .create(
                "Test Account 3",
                "This is a test account",
                Environment::Paper,
                dec!(0.0),
                dec!(0.0),
            )
            .expect("Error creating account");
        let tx = db
            .transaction_write()
            .create_transaction(
                &account,
                dec!(10.99),
                &Currency::BTC,
                TransactionCategory::Deposit,
            )
            .expect("Error creating transaction");

        assert_eq!(tx.account_id, account.id);
        assert_eq!(tx.amount, dec!(10.99));
        assert_eq!(tx.currency, Currency::BTC);
        assert_eq!(tx.category, TransactionCategory::Deposit);
        assert_eq!(tx.deleted_at, None);
    }

    #[test]
    fn test_create_transaction_with_trade_id() {
        let db = create_factory();

        let trade_id = Uuid::new_v4();

        // Create a new account record
        let account = db
            .account_write()
            .create(
                "Test Account 3",
                "This is a test account",
                Environment::Paper,
                dec!(0.0),
                dec!(0.0),
            )
            .expect("Error creating account");
        let tx = db
            .transaction_write()
            .create_transaction(
                &account,
                dec!(10.99),
                &Currency::BTC,
                TransactionCategory::FundTrade(trade_id),
            )
            .expect("Error creating transaction");

        assert_eq!(tx.account_id, account.id);
        assert_eq!(tx.amount, dec!(10.99));
        assert_eq!(tx.currency, Currency::BTC);
        assert_eq!(tx.category, TransactionCategory::FundTrade(trade_id));
        assert_eq!(tx.deleted_at, None);
    }

    #[test]
    fn test_create_transaction_by_account_id() {
        let db: Box<dyn DatabaseFactory> = create_factory();

        let account = db
            .account_write()
            .create(
                "Test Account 4",
                "This is another test account",
                Environment::Paper,
                dec!(0.0),
                dec!(0.0),
            )
            .expect("Error creating account");

        let tx = db
            .transaction_write()
            .create_transaction_by_account_id(
                account.id,
                dec!(42.50),
                &Currency::USD,
                TransactionCategory::Deposit,
            )
            .expect("Error creating transaction by account id");

        assert_eq!(tx.account_id, account.id);
        assert_eq!(tx.amount, dec!(42.50));
        assert_eq!(tx.currency, Currency::USD);
        assert_eq!(tx.category, TransactionCategory::Deposit);
        assert_eq!(tx.deleted_at, None);
    }

    #[test]
    fn account_transaction_queries_filter_active_currency_tax_and_month_boundaries() {
        let (database, connection) = create_database_with_connection();
        let account = create_account(&database, "transaction-query-account");
        let other_account = create_account(&database, "transaction-query-other");
        let first_day = first_day_of_current_month();
        let previous_day = first_day
            .checked_sub_signed(Duration::days(1))
            .expect("previous day should be representable");

        let mut connection = connection
            .lock()
            .expect("connection lock should be acquired");
        let fixture_ids = insert_core_account_query_rows(&mut connection, account.id, previous_day);
        insert_account_query_noise_rows(
            &mut connection,
            account.id,
            other_account.id,
            previous_day,
        );

        assert_active_account_transactions(&mut connection, account.id, &fixture_ids);
        assert_non_tax_account_transactions(&mut connection, account.id, &fixture_ids);
        assert_tax_account_transactions(&mut connection, account.id);
        assert_prior_month_non_tax_transactions(&mut connection, account.id, &fixture_ids);
    }

    #[test]
    fn trade_transaction_queries_filter_by_trade_id_and_category_key() {
        let (database, connection) = create_database_with_connection();
        let account = create_account(&database, "trade-transaction-account");
        let trade = create_persisted_trade(&database, &account, "TXWORKA");
        let other_trade = create_persisted_trade(&database, &account, "TXWORKB");

        let mut connection = connection
            .lock()
            .expect("connection lock should be acquired");
        let open_id = WorkerTransaction::create_transaction(
            &mut connection,
            account.id,
            dec!(1000),
            &Currency::USD,
            TransactionCategory::OpenTrade(trade.id),
        )
        .expect("open trade transaction should be created")
        .id;
        let close_id = WorkerTransaction::create_transaction(
            &mut connection,
            account.id,
            dec!(1200),
            &Currency::USD,
            TransactionCategory::CloseTarget(trade.id),
        )
        .expect("close trade transaction should be created")
        .id;
        WorkerTransaction::create_transaction(
            &mut connection,
            account.id,
            dec!(500),
            &Currency::USD,
            TransactionCategory::OpenTrade(other_trade.id),
        )
        .expect("other trade transaction should be created");

        let trade_transactions =
            WorkerTransaction::read_all_trade_transactions(&mut connection, trade.id)
                .expect("trade transactions should be readable");
        let trade_transaction_ids = transaction_ids(&trade_transactions);
        assert_eq!(trade_transactions.len(), 2);
        assert!(trade_transaction_ids.contains(&open_id));
        assert!(trade_transaction_ids.contains(&close_id));

        let open_transactions = WorkerTransaction::read_all_trade_transactions_for_category(
            &mut connection,
            trade.id,
            TransactionCategory::OpenTrade(Uuid::new_v4()),
        )
        .expect("open trade transactions should be readable");
        assert_eq!(open_transactions.len(), 1);
        assert_eq!(
            open_transactions
                .first()
                .expect("open trade transaction should exist")
                .id,
            open_id
        );
    }

    #[test]
    fn in_trade_account_transactions_include_funded_submitted_and_filled_trades() {
        let (database, connection) = create_database_with_connection();
        let account = create_account(&database, "in-trade-transaction-account");
        let funded = create_persisted_trade(&database, &account, "TXFUNDED");
        let submitted = create_persisted_trade(&database, &account, "TXSUBMIT");
        let filled = create_persisted_trade(&database, &account, "TXFILLED");
        let closed = create_persisted_trade(&database, &account, "TXCLOSED");

        let mut connection = connection
            .lock()
            .expect("connection lock should be acquired");
        set_trade_status(&mut connection, funded.id, Status::Funded);
        set_trade_status(&mut connection, submitted.id, Status::Submitted);
        set_trade_status(&mut connection, filled.id, Status::Filled);
        set_trade_status(&mut connection, closed.id, Status::ClosedTarget);

        let funded_tx_id = WorkerTransaction::create_transaction(
            &mut connection,
            account.id,
            dec!(1000),
            &Currency::USD,
            TransactionCategory::FundTrade(funded.id),
        )
        .expect("funded transaction should be created")
        .id;
        let submitted_tx_id = WorkerTransaction::create_transaction(
            &mut connection,
            account.id,
            dec!(1000),
            &Currency::USD,
            TransactionCategory::OpenTrade(submitted.id),
        )
        .expect("submitted transaction should be created")
        .id;
        let filled_tx_id = WorkerTransaction::create_transaction(
            &mut connection,
            account.id,
            dec!(1000),
            &Currency::USD,
            TransactionCategory::OpenTrade(filled.id),
        )
        .expect("filled transaction should be created")
        .id;
        WorkerTransaction::create_transaction(
            &mut connection,
            account.id,
            dec!(1000),
            &Currency::USD,
            TransactionCategory::OpenTrade(closed.id),
        )
        .expect("closed transaction should be created");

        let in_trade = WorkerTransaction::all_account_transactions_in_trade(
            &mut connection,
            account.id,
            &Currency::USD,
        )
        .expect("in-trade transactions should be readable");

        assert_eq!(
            transaction_ids(&in_trade),
            vec![funded_tx_id, submitted_tx_id, filled_tx_id]
        );
    }

    #[test]
    fn in_trade_account_transactions_propagate_trade_read_errors_for_each_status() {
        for (status, symbol) in [
            (Status::Funded, "TXBADFUND"),
            (Status::Submitted, "TXBADSUB"),
            (Status::Filled, "TXBADFILL"),
        ] {
            let (database, connection) = create_database_with_connection();
            let account = create_account(&database, symbol);
            let trade = create_persisted_trade(&database, &account, symbol);

            let mut connection = connection
                .lock()
                .expect("connection lock should be acquired");
            set_trade_status(&mut connection, trade.id, status);
            soft_delete_trading_vehicle(&mut connection, trade.trading_vehicle.id);

            let error = WorkerTransaction::all_account_transactions_in_trade(
                &mut connection,
                account.id,
                &Currency::USD,
            )
            .expect_err("corrupt matching trade should fail in-trade transaction read");

            assert!(error.to_string().contains("trading_vehicle"));
        }
    }

    #[test]
    fn tax_account_transactions_include_payment_and_withdrawal_tax_categories() {
        let mut connection = establish_connection();
        let account_id = Uuid::new_v4();
        let trade_id = Uuid::new_v4();
        let payment_tax_id = insert_transaction_row(
            &mut connection,
            account_id,
            TransactionCategory::PaymentTax(trade_id),
            Currency::USD,
            dec!(15),
            Utc::now().naive_utc(),
            None,
        );
        let withdrawal_tax_id = insert_transaction_row(
            &mut connection,
            account_id,
            TransactionCategory::WithdrawalTax,
            Currency::USD,
            dec!(-10),
            Utc::now().naive_utc(),
            None,
        );
        insert_transaction_row(
            &mut connection,
            account_id,
            TransactionCategory::Deposit,
            Currency::USD,
            dec!(100),
            Utc::now().naive_utc(),
            None,
        );

        let taxes = WorkerTransaction::read_all_account_transactions_taxes(
            &mut connection,
            account_id,
            &Currency::USD,
        )
        .expect("tax transactions should read");

        assert_eq!(
            transaction_ids(&taxes),
            vec![payment_tax_id, withdrawal_tax_id]
        );
    }

    #[test]
    fn tax_account_transactions_propagate_withdrawal_tax_conversion_errors() {
        let mut connection = establish_connection();
        let account_id = Uuid::new_v4();
        let now = Utc::now().naive_utc();
        let row = NewTransaction {
            id: Uuid::new_v4().to_string(),
            created_at: now,
            updated_at: now,
            deleted_at: None,
            currency: Currency::USD.to_string(),
            category: TransactionCategory::WithdrawalTax.to_string(),
            amount: "not-a-decimal".to_string(),
            account_id: account_id.to_string(),
            trade_id: None,
        };
        insert_into(transactions::table)
            .values(&row)
            .execute(&mut connection)
            .expect("corrupt transaction row should be inserted");

        let error = WorkerTransaction::read_all_account_transactions_taxes(
            &mut connection,
            account_id,
            &Currency::USD,
        )
        .expect_err("corrupt withdrawal tax row should fail tax transaction read");

        assert!(error.to_string().contains("amount"));
    }

    #[test]
    fn transaction_worker_reports_database_errors() {
        let mut connection = establish_connection();
        diesel::sql_query("DROP TABLE transactions")
            .execute(&mut connection)
            .expect("transactions table should drop");
        let account_id = Uuid::new_v4();
        let trade_id = Uuid::new_v4();

        let create_error = WorkerTransaction::create_transaction(
            &mut connection,
            account_id,
            dec!(100),
            &Currency::USD,
            TransactionCategory::Deposit,
        )
        .expect_err("missing table should fail transaction create");
        assert!(create_error.to_string().contains("transactions"));

        let all_error =
            WorkerTransaction::read_all_transactions(&mut connection, account_id, &Currency::USD)
                .expect_err("missing table should fail transaction read");
        assert!(all_error.to_string().contains("transactions"));

        let non_tax_error = WorkerTransaction::read_all_trade_transactions_excluding_taxes(
            &mut connection,
            account_id,
            &Currency::USD,
        )
        .expect_err("missing table should fail non-tax transaction read");
        assert!(non_tax_error.to_string().contains("transactions"));

        let tax_error = WorkerTransaction::read_all_account_transactions_taxes(
            &mut connection,
            account_id,
            &Currency::USD,
        )
        .expect_err("missing table should fail tax transaction read");
        assert!(tax_error.to_string().contains("transactions"));

        let category_error = WorkerTransaction::read_all_account_transactions_for_category(
            &mut connection,
            account_id,
            &Currency::USD,
            TransactionCategory::Deposit,
        )
        .expect_err("missing table should fail account category transaction read");
        assert!(category_error.to_string().contains("transactions"));

        let trade_category_error = WorkerTransaction::read_all_trade_transactions_for_category(
            &mut connection,
            trade_id,
            TransactionCategory::OpenTrade(Uuid::new_v4()),
        )
        .expect_err("missing table should fail trade category transaction read");
        assert!(trade_category_error.to_string().contains("transactions"));

        let trade_error = WorkerTransaction::read_all_trade_transactions(&mut connection, trade_id)
            .expect_err("missing table should fail trade transaction read");
        assert!(trade_error.to_string().contains("transactions"));

        let previous_month_error =
            WorkerTransaction::read_all_transaction_excluding_current_month_and_taxes(
                &mut connection,
                account_id,
                &Currency::USD,
            )
            .expect_err("missing table should fail previous-month transaction read");
        assert!(previous_month_error.to_string().contains("transactions"));
    }

    #[test]
    fn transaction_sqlite_conversion_reports_corrupt_fields() {
        let mut row = transaction_row();
        row.id = "not-a-uuid".to_string();
        assert_transaction_conversion_error(row, "id");

        let mut row = transaction_row();
        row.currency = "not-currency".to_string();
        assert_transaction_conversion_error(row, "currency");

        let mut row = transaction_row();
        row.category = "not-category".to_string();
        assert_transaction_conversion_error(row, "category");

        let mut row = transaction_row();
        row.amount = "not-decimal".to_string();
        assert_transaction_conversion_error(row, "amount");

        let mut row = transaction_row();
        row.account_id = "not-a-uuid".to_string();
        assert_transaction_conversion_error(row, "account_id");

        let mut row = transaction_row();
        row.category = TransactionCategory::FundTrade(Uuid::new_v4()).to_string();
        row.trade_id = Some("not-a-uuid".to_string());
        assert_transaction_conversion_error(row, "category");
    }
}
