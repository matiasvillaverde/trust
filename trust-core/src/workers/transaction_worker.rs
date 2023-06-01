use rust_decimal::Decimal;
use std::error::Error;
use trust_model::{
    AccountOverview, Currency, DatabaseFactory, Trade, TradeOverview, Transaction,
    TransactionCategory,
};
use uuid::Uuid;

use crate::{
    calculators::TradeTransactionsCalculator,
    trade_calculators::TradeCapitalOutOfMarket,
    validators::{TransactionValidationErrorCode, TransactionValidator},
};

use super::OverviewWorker;

pub struct TransactionWorker;

impl TransactionWorker {
    pub fn create(
        database: &mut dyn DatabaseFactory,
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
            TransactionCategory::WithdrawalTax => {
                unimplemented!("WithdrawalTax is not implemented yet")
            }
            TransactionCategory::WithdrawalEarnings => {
                unimplemented!("WithdrawalEarnings is not implemented yet")
            }
            default => {
                let message = format!("Manually creating transaction category {:?} is not allowed. Only Withdrawals and deposits are allowed", default);
                Err(message.into())
            }
        }
    }

    fn deposit(
        database: &mut dyn DatabaseFactory,
        amount: Decimal,
        currency: &Currency,
        account_id: Uuid,
    ) -> Result<(Transaction, AccountOverview), Box<dyn Error>> {
        let account = database.read_account_db().read_account_id(account_id)?;

        match TransactionValidator::validate(
            TransactionCategory::Deposit,
            amount,
            currency,
            account_id,
            database.read_account_overview_db().as_mut(),
            database.read_trade_db().as_mut(),
        ) {
            Ok(_) => {
                let transaction = database.write_transaction_db().create_transaction(
                    &account,
                    amount,
                    currency,
                    TransactionCategory::Deposit,
                )?;
                let updated_overview =
                    OverviewWorker::update_account_overview(database, &account, currency)?;
                Ok((transaction, updated_overview))
            }
            Err(error) => {
                if error.code == TransactionValidationErrorCode::OverviewNotFound {
                    let transaction = database.write_transaction_db().create_transaction(
                        &account,
                        amount,
                        currency,
                        TransactionCategory::Deposit,
                    )?;
                    database
                        .write_account_overview_db()
                        .new_account_overview(&account, currency)?;
                    let updated_overview =
                        OverviewWorker::update_account_overview(database, &account, currency)?;
                    Ok((transaction, updated_overview))
                } else {
                    Err(error)
                }
            }
        }
    }

    fn withdraw(
        database: &mut dyn DatabaseFactory,
        amount: Decimal,
        currency: &Currency,
        account_id: Uuid,
    ) -> Result<(Transaction, AccountOverview), Box<dyn Error>> {
        let account = database.read_account_db().read_account_id(account_id)?;

        // Validate that account has enough funds to withdraw
        TransactionValidator::validate(
            TransactionCategory::Withdrawal,
            amount,
            currency,
            account_id,
            database.read_account_overview_db().as_mut(),
            database.read_trade_db().as_mut(),
        )?;

        // Create transaction
        let transaction = database.write_transaction_db().create_transaction(
            &account,
            amount,
            currency,
            TransactionCategory::Withdrawal,
        )?;

        // Update account overview
        let updated_overview =
            OverviewWorker::update_account_overview(database, &account, currency)?;

        Ok((transaction, updated_overview))
    }

    pub fn transfer_to_fund_trade(
        trade: &Trade,
        database: &mut dyn DatabaseFactory,
    ) -> Result<(Transaction, AccountOverview, TradeOverview), Box<dyn Error>> {
        let account = database
            .read_account_db()
            .read_account_id(trade.account_id)?;

        let trade_total = trade.entry.unit_price.amount * Decimal::from(trade.entry.quantity);

        TransactionValidator::validate(
            TransactionCategory::FundTrade(trade.id),
            trade_total,
            &trade.currency,
            account.id,
            database.read_account_overview_db().as_mut(),
            database.read_trade_db().as_mut(),
        )?;

        let transaction = database.write_transaction_db().create_transaction(
            &account,
            trade_total,
            &trade.currency,
            TransactionCategory::FundTrade(trade.id),
        )?;

        let account_overview =
            OverviewWorker::update_account_overview(database, &account, &trade.currency)?;

        let trade_overview: TradeOverview = OverviewWorker::update_trade_overview(database, trade)?;

        Ok((transaction, account_overview, trade_overview))
    }

    pub fn transfer_to_open_trade(
        trade: &Trade,
        database: &mut dyn DatabaseFactory,
    ) -> Result<(Transaction, TradeOverview), Box<dyn Error>> {
        // TODO: Validate that trade has enough funds to be opened
        let account = database
            .read_account_db()
            .read_account_id(trade.account_id)?;

        let total = trade.entry.unit_price.amount * Decimal::from(trade.entry.quantity);

        let transaction = database.write_transaction_db().create_transaction(
            &account,
            total,
            &trade.currency,
            TransactionCategory::OpenTrade(trade.id),
        )?;

        // Update trade overview
        let trade_overview: TradeOverview = OverviewWorker::update_trade_overview(database, trade)?;

        Ok((transaction, trade_overview))
    }

    pub fn transfer_opening_fee(
        fee: Decimal,
        trade: &Trade,
        database: &mut dyn DatabaseFactory,
    ) -> Result<(Transaction, AccountOverview), Box<dyn Error>> {
        // TODO: Validate that account has enough funds to pay a fee.
        let account = database
            .read_account_db()
            .read_account_id(trade.account_id)?;

        let transaction = database.write_transaction_db().create_transaction(
            &account,
            fee,
            &trade.currency,
            TransactionCategory::FeeOpen(trade.id),
        )?;

        // Update account overview
        let overview =
            OverviewWorker::update_account_overview(database, &account, &trade.currency)?;

        Ok((transaction, overview))
    }

    pub fn transfer_closing_fee(
        fee: Decimal,
        trade: &Trade,
        database: &mut dyn DatabaseFactory,
    ) -> Result<(Transaction, AccountOverview), Box<dyn Error>> {
        // TODO: Validate that account has enough funds to pay a fee.
        let account = database
            .read_account_db()
            .read_account_id(trade.account_id)?;

        let transaction = database.write_transaction_db().create_transaction(
            &account,
            fee,
            &trade.currency,
            TransactionCategory::FeeClose(trade.id),
        )?;

        // Update account overview
        let overview =
            OverviewWorker::update_account_overview(database, &account, &trade.currency)?;

        Ok((transaction, overview))
    }

    pub fn transfer_to_close_target(
        trade: &Trade,
        database: &mut dyn DatabaseFactory,
    ) -> Result<(Transaction, TradeOverview), Box<dyn Error>> {
        // TODO: Validate that trade can be closed

        let account = database
            .read_account_db()
            .read_account_id(trade.account_id)?;

        let total = trade.exit_targets.first().unwrap().order.unit_price.amount
            * Decimal::from(trade.entry.quantity);

        let transaction = database.write_transaction_db().create_transaction(
            &account,
            total,
            &trade.currency,
            TransactionCategory::CloseTarget(trade.id),
        )?;

        // Update trade overview
        let trade_overview: TradeOverview = OverviewWorker::update_trade_overview(database, trade)?;

        Ok((transaction, trade_overview))
    }

    pub fn transfer_to_close_stop(
        trade: &Trade,
        database: &mut dyn DatabaseFactory,
    ) -> Result<(Transaction, TradeOverview), Box<dyn Error>> {
        // TODO: Validate that trade can be closed

        let account = database
            .read_account_db()
            .read_account_id(trade.account_id)?;

        let total = trade.safety_stop.unit_price.amount * Decimal::from(trade.entry.quantity);

        let transaction = database.write_transaction_db().create_transaction(
            &account,
            total,
            &trade.currency,
            TransactionCategory::CloseSafetyStop(trade.id),
        )?;

        // Update trade overview
        let trade_overview: TradeOverview = OverviewWorker::update_trade_overview(database, trade)?;

        Ok((transaction, trade_overview))
    }

    pub fn transfer_payment_from(
        trade: &Trade,
        database: &mut dyn DatabaseFactory,
    ) -> Result<(Transaction, AccountOverview, TradeOverview), Box<dyn Error>> {
        // Create transaction
        let account = database
            .read_account_db()
            .read_account_id(trade.account_id)?;
        let total_to_withdrawal =
            TradeCapitalOutOfMarket::calculate(trade.id, database.read_transaction_db().as_mut())?;

        let transaction = database.write_transaction_db().create_transaction(
            &account,
            total_to_withdrawal,
            &trade.currency,
            TransactionCategory::PaymentFromTrade(trade.id),
        )?;

        // Update account overview and trade overview.
        let account_overview: AccountOverview =
            OverviewWorker::update_account_overview(database, &account, &trade.currency)?;
        let trade_overview: TradeOverview = OverviewWorker::update_trade_overview(database, trade)?;

        Ok((transaction, account_overview, trade_overview))
    }
}
