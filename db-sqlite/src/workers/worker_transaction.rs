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
        let now = Utc::now().naive_utc();

        let new_transaction = NewTransaction {
            id: Uuid::new_v4().to_string(),
            created_at: now,
            updated_at: now,
            deleted_at: None,
            currency: currency.to_string(),
            category: category.to_string(),
            account_id: account_id.to_string(),
            amount: amount.to_string(),
            trade_id: category.trade_id().map(|uuid| uuid.to_string()),
        };

        let transaction = diesel::insert_into(transactions::table)
            .values(&new_transaction)
            .get_result::<TransactionSQLite>(connection)
            .map(|tx| tx.domain_model())
            .map_err(|error| {
                error!("Error creating transaction: {:?}", error);
                error
            })?;
        Ok(transaction)
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
            .map(|transactions: Vec<TransactionSQLite>| {
                transactions
                    .into_iter()
                    .map(|tx| tx.domain_model())
                    .collect()
            })
            .map_err(|error| {
                error!("Error reading all transactions: {:?}", error);
                error
            })?;
        Ok(transactions)
    }

    pub fn read_all_trade_transactions_excluding_taxes(
        connection: &mut SqliteConnection,
        account_id: Uuid,
        currency: &Currency,
    ) -> Result<Vec<Transaction>, Box<dyn Error>> {
        // REFACTOR: Query all transactions for an account and filer taxes out in memory.
        let tx_deposit = WorkerTransaction::read_all_account_transactions_for_category(
            connection,
            account_id,
            currency,
            TransactionCategory::Deposit,
        )?;
        let tx_withdrawal = WorkerTransaction::read_all_account_transactions_for_category(
            connection,
            account_id,
            currency,
            TransactionCategory::Withdrawal,
        )?;

        let tx_fee_open = WorkerTransaction::read_all_account_transactions_for_category(
            connection,
            account_id,
            currency,
            TransactionCategory::FeeOpen(Uuid::new_v4()),
        )?;

        let tx_fee_close = WorkerTransaction::read_all_account_transactions_for_category(
            connection,
            account_id,
            currency,
            TransactionCategory::FeeClose(Uuid::new_v4()),
        )?;

        let tx_output = WorkerTransaction::read_all_account_transactions_for_category(
            connection,
            account_id,
            currency,
            TransactionCategory::FundTrade(Uuid::new_v4()),
        )?;

        let tx_input = WorkerTransaction::read_all_account_transactions_for_category(
            connection,
            account_id,
            currency,
            TransactionCategory::PaymentFromTrade(Uuid::new_v4()),
        )?;
        Ok(tx_deposit
            .into_iter()
            .chain(tx_withdrawal)
            .chain(tx_fee_open)
            .chain(tx_fee_close)
            .chain(tx_output)
            .chain(tx_input)
            .collect())
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
            .load::<TransactionSQLite>(connection)
            .map(|transactions: Vec<TransactionSQLite>| {
                transactions
                    .into_iter()
                    .map(|tx| tx.domain_model())
                    .collect()
            })
            .map_err(|error| {
                error!("Error reading transactions: {:?}", error);
                error
            })?;
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
            .load::<TransactionSQLite>(connection)
            .map(|transactions: Vec<TransactionSQLite>| {
                transactions
                    .into_iter()
                    .map(|tx| tx.domain_model())
                    .collect()
            })
            .map_err(|error| {
                error!("Error creating price: {:?}", error);
                error
            })?;
        Ok(transactions)
    }

    pub fn read_all_trade_transactions(
        connection: &mut SqliteConnection,
        trade: Uuid,
    ) -> Result<Vec<Transaction>, Box<dyn Error>> {
        let transactions = transactions::table
            .filter(transactions::deleted_at.is_null())
            .filter(transactions::trade_id.eq(trade.to_string()))
            .load::<TransactionSQLite>(connection)
            .map(|transactions: Vec<TransactionSQLite>| {
                transactions
                    .into_iter()
                    .map(|tx| tx.domain_model())
                    .collect()
            })
            .map_err(|error| {
                error!("Error reading trade transactions: {:?}", error);
                error
            })?;
        Ok(transactions)
    }

    pub fn read_all_transaction_excluding_current_month_and_taxes(
        connection: &mut SqliteConnection,
        account_id: Uuid,
        currency: &Currency,
    ) -> Result<Vec<Transaction>, Box<dyn Error>> {
        let tx_deposits = WorkerTransaction::read_all_transaction_beginning_of_the_month(
            connection,
            account_id,
            currency,
            TransactionCategory::Deposit,
        )?;
        let tx_withdrawals = WorkerTransaction::read_all_transaction_beginning_of_the_month(
            connection,
            account_id,
            currency,
            TransactionCategory::Withdrawal,
        )?;
        let tx_outputs = WorkerTransaction::read_all_transaction_beginning_of_the_month(
            connection,
            account_id,
            currency,
            TransactionCategory::FundTrade(Uuid::new_v4()),
        )?;
        let tx_inputs = WorkerTransaction::read_all_transaction_beginning_of_the_month(
            connection,
            account_id,
            currency,
            TransactionCategory::PaymentFromTrade(Uuid::new_v4()),
        )?;

        Ok(tx_deposits
            .into_iter()
            .chain(tx_withdrawals)
            .chain(tx_outputs)
            .chain(tx_inputs)
            .collect())
    }

    fn read_all_transaction_beginning_of_the_month(
        connection: &mut SqliteConnection,
        account_id: Uuid,
        currency: &Currency,
        category: TransactionCategory,
    ) -> Result<Vec<Transaction>, Box<dyn Error>> {
        let now = Utc::now().naive_utc();
        let first_day_of_month =
            NaiveDate::from_ymd_opt(now.year(), now.month(), 1).ok_or("Failed to create date")?;
        let first_day_of_month = NaiveDateTime::new(
            first_day_of_month,
            NaiveTime::from_hms_opt(0, 0, 0).ok_or("Failed to create time")?,
        );

        let tx = transactions::table
            .filter(transactions::deleted_at.is_null())
            .filter(transactions::account_id.eq(account_id.to_string()))
            .filter(transactions::created_at.le(first_day_of_month))
            .filter(transactions::currency.eq(currency.to_string()))
            .filter(transactions::category.eq(category.key()))
            .load::<TransactionSQLite>(connection)
            .map(|transactions: Vec<TransactionSQLite>| {
                transactions
                    .into_iter()
                    .map(|tx| tx.domain_model())
                    .collect()
            })
            .map_err(|error| {
                error!("Error creating price: {:?}", error);
                error
            })?;
        Ok(tx)
    }
}

#[derive(Queryable, Identifiable, AsChangeset, Insertable)]
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

impl TransactionSQLite {
    fn domain_model(&self) -> Transaction {
        let trade_id = self
            .trade_id
            .clone()
            .and_then(|uuid| Uuid::parse_str(&uuid).ok());
        let category = TransactionCategory::parse(&self.category, trade_id).unwrap_or_else(|_| {
            eprintln!("Failed to parse transaction category: {}", &self.category);
            std::process::exit(1);
        });

        Transaction {
            id: Uuid::parse_str(&self.id).unwrap_or_else(|e| {
                eprintln!("Failed to parse transaction ID: {}", e);
                std::process::exit(1);
            }),
            created_at: self.created_at,
            updated_at: self.updated_at,
            deleted_at: self.deleted_at,
            category,
            currency: Currency::from_str(&self.currency).unwrap_or_else(|_| {
                eprintln!("Failed to parse currency: {}", &self.currency);
                std::process::exit(1);
            }),
            amount: Decimal::from_str(&self.amount).unwrap_or_else(|e| {
                eprintln!("Failed to parse amount: {}", e);
                std::process::exit(1);
            }),
            account_id: Uuid::parse_str(&self.account_id).unwrap_or_else(|e| {
                eprintln!("Failed to parse account ID: {}", e);
                std::process::exit(1);
            }),
        }
    }
}
#[derive(Insertable)]
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
    use rust_decimal_macros::dec;

    use super::*;
    use crate::SqliteDatabase;
    use diesel_migrations::*;
    use model::{DatabaseFactory, Environment};
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
}
