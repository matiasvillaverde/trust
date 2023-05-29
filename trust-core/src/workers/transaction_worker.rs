// Validate that the transaction is possible
// Create transaction
// Update Account Overview

use rust_decimal::Decimal;
use std::error::Error;
use trust_model::{AccountOverview, Currency, Database, Trade, Transaction, TransactionCategory};
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
            _default => {
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

    pub fn transfer_to_trade(
        trade: &Trade,
        database: &mut dyn Database,
    ) -> Result<(Transaction, AccountOverview), Box<dyn Error>> {
        let account = database.read_account_id(trade.account_id)?;
        let overview = database.read_account_overview_currency(account.id, &trade.currency)?;
        let trade_total = trade.entry.unit_price.amount * Decimal::from(trade.entry.quantity);
        let total_available = overview.total_available.amount - trade_total;
        let total_in_trade = overview.total_in_trade.amount + trade_total;

        match TransactionValidator::validate(
            TransactionCategory::FundTrade(trade.id),
            trade_total,
            &trade.currency,
            account.id,
            database,
        ) {
            Ok(_) => {
                let updated_overview = database.update_account_overview_trade(
                    &account,
                    &trade.currency,
                    total_available,
                    total_in_trade,
                )?;

                _ = database.update_trade_overview(trade, trade_total);

                let transaction = database.new_transaction(
                    &account,
                    trade_total,
                    &trade.currency,
                    TransactionCategory::FundTrade(trade.id),
                )?;

                Ok((transaction, updated_overview))
            }
            Err(error) => Err(error),
        }
    }

    pub fn transfer_out_trade(
        trade: &Trade,
        database: &mut dyn Database,
    ) -> Result<(Transaction, AccountOverview), Box<dyn Error>> {
        let account = database.read_account_id(trade.account_id)?;
        let overview = database.read_account_overview_currency(account.id, &trade.currency)?;

        let total_trade = trade.overview.total_out_market.amount;
        let total_available = overview.total_available.amount + total_trade;
        let total_in_trade = overview.total_in_trade.amount - total_trade;

        let updated_overview = database.update_account_overview_trade(
            &account,
            &trade.currency,
            total_available,
            total_in_trade,
        )?;

        let transaction = database.new_transaction(
            &account,
            total_trade,
            &trade.currency,
            TransactionCategory::PaymentFromTrade(trade.id),
        )?;

        Ok((transaction, updated_overview))
    }
}
