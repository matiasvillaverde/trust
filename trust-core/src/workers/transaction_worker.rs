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

use super::OverviewWorker;

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
                let updated_overview =
                    OverviewWorker::update_account_overview(database, &account, currency)?;
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

                let updated_overview =
                    OverviewWorker::update_account_overview(database, &account, currency)?;
                Ok((transaction, updated_overview))
            }
            Err(error) => Err(error),
        }
    }

    pub fn transfer_to_fund_trade(
        trade: &Trade,
        database: &mut dyn Database,
    ) -> Result<(Transaction, AccountOverview, TradeOverview), Box<dyn Error>> {
        let account = database.read_account_id(trade.account_id)?;

        let trade_total = trade.entry.unit_price.amount * Decimal::from(trade.entry.quantity);

        match TransactionValidator::validate(
            TransactionCategory::FundTrade(trade.id),
            trade_total,
            &trade.currency,
            account.id,
            database,
        ) {
            Ok(_) => {
                let transaction = database.new_transaction(
                    &account,
                    trade_total,
                    &trade.currency,
                    TransactionCategory::FundTrade(trade.id),
                )?;

                let account_overview =
                    OverviewWorker::update_account_overview(database, &account, &trade.currency)?;

                let trade_overview: TradeOverview =
                    OverviewWorker::update_trade_overview(database, trade)?;

                Ok((transaction, account_overview, trade_overview))
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
        let trade_overview: TradeOverview = OverviewWorker::update_trade_overview(database, trade)?;

        Ok((transaction, trade_overview))
    }

    pub fn transfer_fee(
        fee: Decimal,
        trade: &Trade,
        database: &mut dyn Database,
    ) -> Result<(Transaction, AccountOverview), Box<dyn Error>> {
        // TODO: Validate that account has enough funds to pay a fee.
        let account = database.read_account_id(trade.account_id)?;

        let transaction = database.new_transaction(
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

    pub fn transfer_to_close_target(
        trade: &Trade,
        database: &mut dyn Database,
    ) -> Result<(Transaction, TradeOverview), Box<dyn Error>> {
        // TODO: Validate that trade can be closed

        let account = database.read_account_id(trade.account_id)?;

        let total = trade.exit_targets.first().unwrap().order.unit_price.amount
            * Decimal::from(trade.entry.quantity);

        let transaction = database.new_transaction(
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
        database: &mut dyn Database,
    ) -> Result<(Transaction, TradeOverview), Box<dyn Error>> {
        // TODO: Validate that trade can be closed

        let account = database.read_account_id(trade.account_id)?;

        let total = trade.safety_stop.unit_price.amount * Decimal::from(trade.entry.quantity);

        let transaction = database.new_transaction(
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
        database: &mut dyn Database,
    ) -> Result<(Transaction, AccountOverview, TradeOverview), Box<dyn Error>> {
        // Create transaction
        let account = database.read_account_id(trade.account_id)?;
        let total_to_withdrawal = TransactionsCalculator::capital_out_of_market(trade, database)?;

        let transaction = database.new_transaction(
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
