use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use std::error::Error;
use model::{
    AccountOverview, Currency, DatabaseFactory, Trade, TradeOverview, Transaction,
    TransactionCategory,
};
use uuid::Uuid;

use crate::{
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
        ) {
            Ok(_) => {
                let transaction = database.write_transaction_db().create_transaction(
                    &account,
                    amount,
                    currency,
                    TransactionCategory::Deposit,
                )?;
                let updated_overview =
                    OverviewWorker::calculate_account(database, &account, currency)?;
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
                        .create_account_overview(&account, currency)?;
                    let updated_overview =
                        OverviewWorker::calculate_account(database, &account, currency)?;
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
        )?;

        // Create transaction
        let transaction = database.write_transaction_db().create_transaction(
            &account,
            amount,
            currency,
            TransactionCategory::Withdrawal,
        )?;

        // Update account overview
        let updated_overview = OverviewWorker::calculate_account(database, &account, currency)?;

        Ok((transaction, updated_overview))
    }

    pub fn transfer_to_fund_trade(
        trade: &Trade,
        database: &mut dyn DatabaseFactory,
    ) -> Result<(Transaction, AccountOverview, TradeOverview), Box<dyn Error>> {
        // 1. Validate that trade can be fund
        crate::validators::funding_validator::can_fund(trade, database)?;

        // 2. Create transaction
        let account = database
            .read_account_db()
            .read_account_id(trade.account_id)?;

        let trade_total = trade.entry.unit_price * Decimal::from(trade.entry.quantity);

        let transaction = database.write_transaction_db().create_transaction(
            &account,
            trade_total,
            &trade.currency,
            TransactionCategory::FundTrade(trade.id),
        )?;

        // 3. Update Account Overview and Trade Overview
        let account_overview =
            OverviewWorker::calculate_account(database, &account, &trade.currency)?;
        let trade_overview: TradeOverview = OverviewWorker::calculate_trade(database, trade)?;

        Ok((transaction, account_overview, trade_overview))
    }

    pub fn transfer_to_fill_trade(
        trade: &Trade,
        database: &mut dyn DatabaseFactory,
    ) -> Result<(Transaction, TradeOverview), Box<dyn Error>> {
        let account = database
            .read_account_db()
            .read_account_id(trade.account_id)?;

        // 1. Calculate the total amount of the trade
        let total = trade.entry.average_filled_price.unwrap() * Decimal::from(trade.entry.quantity);

        // 2. Validate that the trade has enough funds to fill the trade
        TransactionValidator::validate_fill(trade, total)?;

        // 3. Create transaction
        let transaction = database.write_transaction_db().create_transaction(
            &account,
            total,
            &trade.currency,
            TransactionCategory::OpenTrade(trade.id),
        )?;

        // 4. If there is a difference between the unit_price and the average_filled_price
        // then we should create a transaction to transfer the difference to the account.
        let mut total_difference =
            total - trade.entry.unit_price * Decimal::from(trade.entry.quantity);
        total_difference.set_sign_positive(true);

        if total_difference > dec!(0) {
            database.write_transaction_db().create_transaction(
                &account,
                total_difference,
                &trade.currency,
                TransactionCategory::PaymentFromTrade(trade.id),
            )?;
        }

        // 5. Update trade overview
        let trade_overview: TradeOverview = OverviewWorker::calculate_trade(database, trade)?;
        Ok((transaction, trade_overview))
    }

    pub fn transfer_opening_fee(
        fee: Decimal,
        trade: &Trade,
        database: &mut dyn DatabaseFactory,
    ) -> Result<(Transaction, AccountOverview), Box<dyn Error>> {
        // 1. Validate that account has enough funds to pay a fee.
        let account_overview = database
            .read_account_overview_db()
            .read_account_overview_currency(trade.account_id, &trade.currency)?;
        TransactionValidator::validate_fee(&account_overview, fee)?;

        // 2. Create transaction
        let account = database
            .read_account_db()
            .read_account_id(trade.account_id)?;
        let transaction = database.write_transaction_db().create_transaction(
            &account,
            fee,
            &trade.currency,
            TransactionCategory::FeeOpen(trade.id),
        )?;

        // 3. Update account overview
        let overview = OverviewWorker::calculate_account(database, &account, &trade.currency)?;

        Ok((transaction, overview))
    }

    pub fn transfer_closing_fee(
        fee: Decimal,
        trade: &Trade,
        database: &mut dyn DatabaseFactory,
    ) -> Result<(Transaction, AccountOverview), Box<dyn Error>> {
        // 1. Validate that account has enough funds to pay a fee.
        let account_overview = database
            .read_account_overview_db()
            .read_account_overview_currency(trade.account_id, &trade.currency)?;
        TransactionValidator::validate_fee(&account_overview, fee)?;

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
        let overview = OverviewWorker::calculate_account(database, &account, &trade.currency)?;

        Ok((transaction, overview))
    }

    pub fn transfer_to_close_target(
        trade: &Trade,
        database: &mut dyn DatabaseFactory,
    ) -> Result<(Transaction, TradeOverview), Box<dyn Error>> {
        let account = database
            .read_account_db()
            .read_account_id(trade.account_id)?;

        let total =
            trade.target.average_filled_price.unwrap() * Decimal::from(trade.entry.quantity);

        // 1. Validate that the closing is possible
        TransactionValidator::validate_close(total)?;

        // 2. Create transaction
        let transaction = database.write_transaction_db().create_transaction(
            &account,
            total,
            &trade.currency,
            TransactionCategory::CloseTarget(trade.id),
        )?;

        // 3. Update trade overview and account overview
        let trade_overview: TradeOverview = OverviewWorker::calculate_trade(database, trade)?;
        OverviewWorker::calculate_account(database, &account, &trade.currency)?;

        Ok((transaction, trade_overview))
    }

    pub fn transfer_to_close_stop(
        trade: &Trade,
        database: &mut dyn DatabaseFactory,
    ) -> Result<(Transaction, TradeOverview), Box<dyn Error>> {
        let account = database
            .read_account_db()
            .read_account_id(trade.account_id)?;

        // 1. Calculate the total amount of the trade
        let total =
            trade.safety_stop.average_filled_price.unwrap() * Decimal::from(trade.entry.quantity);

        // 2. Validate that the closing is possible
        TransactionValidator::validate_close(total)?;

        // 3. If the stop was lower than the planned price, then we should create a transaction
        // with category slippage. For more information see: https://www.investopedia.com/terms/s/slippage.asp
        let category = if total > trade.safety_stop.unit_price * Decimal::from(trade.entry.quantity)
        {
            TransactionCategory::CloseSafetyStopSlippage(trade.id)
        } else {
            TransactionCategory::CloseSafetyStop(trade.id)
        };

        // 4. Create transaction
        let transaction = database.write_transaction_db().create_transaction(
            &account,
            total,
            &trade.currency,
            category,
        )?;

        // 5. Update trade overview and account overview
        let trade_overview: TradeOverview = OverviewWorker::calculate_trade(database, trade)?;
        OverviewWorker::calculate_account(database, &account, &trade.currency)?;

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
            OverviewWorker::calculate_account(database, &account, &trade.currency)?;
        let trade_overview: TradeOverview = OverviewWorker::calculate_trade(database, trade)?;

        Ok((transaction, account_overview, trade_overview))
    }
}
