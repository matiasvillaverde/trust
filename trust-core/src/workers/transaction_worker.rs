use rust_decimal::Decimal;
use std::error::Error;
use trust_model::{
    AccountOverview, Currency, Database, Trade, TradeOverview, Transaction, TransactionCategory,
};
use uuid::Uuid;

use crate::{
    calculators::TransactionsCalculator,
    validators::{TransactionValidationErrorCode, TransactionValidator},
};

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

    pub fn transfer_to_open_trade(
        trade: &Trade,
        database: &mut dyn Database,
    ) -> Result<(Transaction, TradeOverview), Box<dyn Error>> {
        // TODO: Validate that trade has enough funds to be opened
        let account = database.read_account_id(trade.account_id)?;

        let total = trade.entry.unit_price.amount * Decimal::from(trade.entry.quantity);

        let transaction = database.new_transaction(
            &account,
            total,
            &trade.currency,
            TransactionCategory::OpenTrade(trade.id),
        )?;

        // Update trade overview
        let overview = database.update_trade_overview_in(trade, total)?;

        return Ok((transaction, overview));
    }

    pub fn transfer_to_close_target(
        trade: &Trade,
        database: &mut dyn Database,
    ) -> Result<(Transaction, TradeOverview), Box<dyn Error>> {
        // TODO: Validate that trade can be closed

        let account = database.read_account_id(trade.account_id)?;

        let total = trade.exit_targets.first().unwrap().order.unit_price.amount
            * Decimal::from(trade.entry.quantity);
        let total_taxable = Decimal::from(0); // TODO: Calculate taxes
        let total_performance =
            total - (trade.entry.unit_price.amount * Decimal::from(trade.entry.quantity));

        let transaction = database.new_transaction(
            &account,
            total,
            &trade.currency,
            TransactionCategory::CloseTarget(trade.id),
        )?;

        // Update trade overview
        let overview =
            database.update_trade_overview_out(trade, total, total_taxable, total_performance)?;

        return Ok((transaction, overview));
    }

    pub fn transfer_payment_from(
        trade: &Trade,
        database: &mut dyn Database,
    ) -> Result<(Transaction, AccountOverview), Box<dyn Error>> {
        let account = database.read_account_id(trade.account_id)?;
        let overview = database.read_account_overview_currency(account.id, &trade.currency)?;

        let total_to_withdrawal =
            TransactionsCalculator::calculate_total_out_of_market_from(trade, database)?;
        let total_available_in_account = TransactionsCalculator::calculate_total_capital_available(
            trade.account_id,
            &trade.currency,
            database,
        )?;

        let new_total_available = total_available_in_account + total_to_withdrawal;
        let total_in_trade = overview.total_in_trade.amount - total_to_withdrawal; // TODO: use transactions to calculate total in trade

        let updated_overview = database.update_account_overview_trade(
            &account,
            &trade.currency,
            new_total_available,
            total_in_trade,
        )?;

        let transaction = database.new_transaction(
            &account,
            total_to_withdrawal,
            &trade.currency,
            TransactionCategory::PaymentFromTrade(trade.id),
        )?;

        Ok((transaction, updated_overview))
    }
}
