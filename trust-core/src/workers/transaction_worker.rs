// Validate that the transaction is possible
// Create transaction
// Update Account Overview

use rust_decimal::Decimal;
use std::error::Error;
use trust_model::{AccountOverview, Currency, Database, Transaction, TransactionCategory};
use uuid::Uuid;

use crate::validators::{TransactionValidationErrorCode, TransactionValidator};

pub struct TransactionWorker;

impl TransactionWorker {
    pub fn create(
        database: &mut dyn Database,
        category: &TransactionCategory,
        amount: Decimal,
        currency: &Currency,
        account_id: Uuid,
    ) -> Result<(Transaction, AccountOverview), Box<dyn Error>> {
        match category {
            TransactionCategory::Deposit => {
                return Self::deposit(database, amount, currency, account_id);
            }
            TransactionCategory::Withdrawal => {
                return Self::withdraw(database, amount, currency, account_id);
            }
            TransactionCategory::Input(_)
            | TransactionCategory::Output(_)
            | TransactionCategory::InputTax(_)
            | TransactionCategory::OutputTax => {
                unimplemented!("Withdrawal is not implemented yet");
            }
        }
    }

    fn deposit(
        database: &mut dyn Database,
        amount: Decimal,
        currency: &Currency,
        account_id: Uuid,
    ) -> Result<(Transaction, AccountOverview), Box<dyn Error>> {
        let account = database.read_account_id(account_id)?;

        match TransactionValidator::validate(
            TransactionCategory::Deposit,
            amount,
            currency,
            account_id,
            database,
        ) {
            Ok(_) => {
                let transaction = database.new_transaction(
                    &account,
                    amount,
                    currency,
                    TransactionCategory::Deposit,
                )?;
                let overview = database.read_account_overview_currency(account.id, currency)?;
                let total_available = overview.total_available.amount + amount;
                let total_balance = overview.total_balance.amount + amount;
                let updated_overview = database.update_account_overview(
                    &account,
                    currency,
                    total_available,
                    total_balance,
                )?;
                Ok((transaction, updated_overview))
            }
            Err(error) => {
                if error.code == TransactionValidationErrorCode::OverviewNotFound {
                    let transaction = database.new_transaction(
                        &account,
                        amount,
                        currency,
                        TransactionCategory::Deposit,
                    )?;
                    database.new_account_overview(&account, currency)?;
                    let overview =
                        database.update_account_overview(&account, currency, amount, amount)?;
                    Ok((transaction, overview))
                } else {
                    Err(error)
                }
            }
        }
    }

    fn withdraw(
        database: &mut dyn Database,
        amount: Decimal,
        currency: &Currency,
        account_id: Uuid,
    ) -> Result<(Transaction, AccountOverview), Box<dyn Error>> {
        let account = database.read_account_id(account_id)?;

        match TransactionValidator::validate(
            TransactionCategory::Withdrawal,
            amount,
            currency,
            account_id,
            database,
        ) {
            Ok(_) => {
                let transaction = database.new_transaction(
                    &account,
                    amount,
                    currency,
                    TransactionCategory::Withdrawal,
                )?;
                let overview = database.read_account_overview_currency(account.id, currency)?;
                let total_available = overview.total_available.amount - amount;
                let total_balance = overview.total_balance.amount - amount;
                let updated_overview = database.update_account_overview(
                    &account,
                    currency,
                    total_available,
                    total_balance,
                )?;
                Ok((transaction, updated_overview))
            }
            Err(error) => Err(error),
        }
    }
}