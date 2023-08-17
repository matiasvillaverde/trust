use model::{
    AccountBalance, Currency, DatabaseFactory, Trade, TradeBalance, Transaction,
    TransactionCategory,
};
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use std::error::Error;
use uuid::Uuid;

use crate::{
    calculators_trade::TradeCapitalOutOfMarket,
    validators::{
        transaction::{self, can_transfer_deposit},
        TransactionValidationErrorCode,
    },
};

use super::balance;

pub fn create(
    database: &mut dyn DatabaseFactory,
    category: &TransactionCategory,
    amount: Decimal,
    currency: &Currency,
    account_id: Uuid,
) -> Result<(Transaction, AccountBalance), Box<dyn Error>> {
    match category {
        TransactionCategory::Deposit => {
            return deposit(database, amount, currency, account_id);
        }
        TransactionCategory::Withdrawal => {
            return withdraw(database, amount, currency, account_id);
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
) -> Result<(Transaction, AccountBalance), Box<dyn Error>> {
    let account = database.account_read().id(account_id)?;

    match can_transfer_deposit(
        amount,
        currency,
        account_id,
        database.account_balance_read().as_mut(),
    ) {
        Ok(_) => {
            let transaction = database.transaction_write().create_transaction(
                &account,
                amount,
                currency,
                TransactionCategory::Deposit,
            )?;
            let updated_balance = balance::calculate_account(database, &account, currency)?;
            Ok((transaction, updated_balance))
        }
        Err(error) => {
            if error.code == TransactionValidationErrorCode::OverviewNotFound {
                let transaction = database.transaction_write().create_transaction(
                    &account,
                    amount,
                    currency,
                    TransactionCategory::Deposit,
                )?;
                database
                    .account_balance_write()
                    .create(&account, currency)?;
                let updated_balance = balance::calculate_account(database, &account, currency)?;
                Ok((transaction, updated_balance))
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
) -> Result<(Transaction, AccountBalance), Box<dyn Error>> {
    let account = database.account_read().id(account_id)?;

    // Validate that account has enough funds to withdraw
    transaction::can_transfer_withdraw(
        amount,
        currency,
        account_id,
        database.account_balance_read().as_mut(),
    )?;

    // Create transaction
    let transaction = database.transaction_write().create_transaction(
        &account,
        amount,
        currency,
        TransactionCategory::Withdrawal,
    )?;

    // Update account balance
    let updated_balance = balance::calculate_account(database, &account, currency)?;

    Ok((transaction, updated_balance))
}

pub fn transfer_to_fund_trade(
    trade: &Trade,
    database: &mut dyn DatabaseFactory,
) -> Result<(Transaction, AccountBalance, TradeBalance), Box<dyn Error>> {
    // 1. Validate that trade can be fund
    crate::validators::funding::can_fund(trade, database)?;

    // 2. Create transaction
    let account = database.account_read().id(trade.account_id)?;

    let trade_total = trade.entry.unit_price * Decimal::from(trade.entry.quantity);

    let transaction = database.transaction_write().create_transaction(
        &account,
        trade_total,
        &trade.currency,
        TransactionCategory::FundTrade(trade.id),
    )?;

    // 3. Update Account Overview and Trade Overview
    let account_balance = balance::calculate_account(database, &account, &trade.currency)?;
    let trade_balance: TradeBalance = balance::calculate_trade(database, trade)?;

    Ok((transaction, account_balance, trade_balance))
}

pub fn transfer_to_fill_trade(
    trade: &Trade,
    database: &mut dyn DatabaseFactory,
) -> Result<(Transaction, TradeBalance), Box<dyn Error>> {
    let account = database.account_read().id(trade.account_id)?;

    // 1. Calculate the total amount of the trade
    let total = trade.entry.average_filled_price.unwrap() * Decimal::from(trade.entry.quantity);

    // 2. Validate that the trade has enough funds to fill the trade
    transaction::can_transfer_fill(trade, total)?;

    // 3. Create transaction
    let transaction = database.transaction_write().create_transaction(
        &account,
        total,
        &trade.currency,
        TransactionCategory::OpenTrade(trade.id),
    )?;

    // 4. If there is a difference between the unit_price and the average_filled_price
    // then we should create a transaction to transfer the difference to the account.
    let mut total_difference = total - trade.entry.unit_price * Decimal::from(trade.entry.quantity);
    total_difference.set_sign_positive(true);

    if total_difference > dec!(0) {
        database.transaction_write().create_transaction(
            &account,
            total_difference,
            &trade.currency,
            TransactionCategory::PaymentFromTrade(trade.id),
        )?;
    }

    // 5. Update trade balance
    let trade_balance: TradeBalance = balance::calculate_trade(database, trade)?;
    Ok((transaction, trade_balance))
}

pub fn transfer_opening_fee(
    fee: Decimal,
    trade: &Trade,
    database: &mut dyn DatabaseFactory,
) -> Result<(Transaction, AccountBalance), Box<dyn Error>> {
    // 1. Validate that account has enough funds to pay a fee.
    let account_balance = database
        .account_balance_read()
        .for_currency(trade.account_id, &trade.currency)?;
    transaction::can_transfer_fee(&account_balance, fee)?;

    // 2. Create transaction
    let account = database.account_read().id(trade.account_id)?;
    let transaction = database.transaction_write().create_transaction(
        &account,
        fee,
        &trade.currency,
        TransactionCategory::FeeOpen(trade.id),
    )?;

    // 3. Update account balance
    let balance = balance::calculate_account(database, &account, &trade.currency)?;

    Ok((transaction, balance))
}

pub fn transfer_closing_fee(
    fee: Decimal,
    trade: &Trade,
    database: &mut dyn DatabaseFactory,
) -> Result<(Transaction, AccountBalance), Box<dyn Error>> {
    // 1. Validate that account has enough funds to pay a fee.
    let account_balance = database
        .account_balance_read()
        .for_currency(trade.account_id, &trade.currency)?;
    transaction::can_transfer_fee(&account_balance, fee)?;

    let account = database.account_read().id(trade.account_id)?;

    let transaction = database.transaction_write().create_transaction(
        &account,
        fee,
        &trade.currency,
        TransactionCategory::FeeClose(trade.id),
    )?;

    // Update account balance
    let balance = balance::calculate_account(database, &account, &trade.currency)?;

    Ok((transaction, balance))
}

pub fn transfer_to_close_target(
    trade: &Trade,
    database: &mut dyn DatabaseFactory,
) -> Result<(Transaction, TradeBalance), Box<dyn Error>> {
    let account = database.account_read().id(trade.account_id)?;

    let total = trade.target.average_filled_price.unwrap() * Decimal::from(trade.entry.quantity);

    // 1. Validate that the closing is possible
    transaction::can_transfer_close(total)?;

    // 2. Create transaction
    let transaction = database.transaction_write().create_transaction(
        &account,
        total,
        &trade.currency,
        TransactionCategory::CloseTarget(trade.id),
    )?;

    // 3. Update trade balance and account balance
    let trade_balance: TradeBalance = balance::calculate_trade(database, trade)?;
    balance::calculate_account(database, &account, &trade.currency)?;

    Ok((transaction, trade_balance))
}

pub fn transfer_to_close_stop(
    trade: &Trade,
    database: &mut dyn DatabaseFactory,
) -> Result<(Transaction, TradeBalance), Box<dyn Error>> {
    let account = database.account_read().id(trade.account_id)?;

    // 1. Calculate the total amount of the trade
    let total =
        trade.safety_stop.average_filled_price.unwrap() * Decimal::from(trade.entry.quantity);

    // 2. Validate that the closing is possible
    transaction::can_transfer_close(total)?;

    // 3. If the stop was lower than the planned price, then we should create a transaction
    // with category slippage. For more information see: https://www.investopedia.com/terms/s/slippage.asp
    let category = if total > trade.safety_stop.unit_price * Decimal::from(trade.entry.quantity) {
        TransactionCategory::CloseSafetyStopSlippage(trade.id)
    } else {
        TransactionCategory::CloseSafetyStop(trade.id)
    };

    // 4. Create transaction
    let transaction = database.transaction_write().create_transaction(
        &account,
        total,
        &trade.currency,
        category,
    )?;

    // 5. Update trade balance and account balance
    let trade_balance: TradeBalance = balance::calculate_trade(database, trade)?;
    balance::calculate_account(database, &account, &trade.currency)?;

    Ok((transaction, trade_balance))
}

pub fn transfer_to_account_from(
    trade: &Trade,
    database: &mut dyn DatabaseFactory,
) -> Result<(Transaction, AccountBalance, TradeBalance), Box<dyn Error>> {
    // Create transaction
    let account = database.account_read().id(trade.account_id)?;
    let total_to_withdrawal =
        TradeCapitalOutOfMarket::calculate(trade.id, database.transaction_read().as_mut())?;

    let transaction = database.transaction_write().create_transaction(
        &account,
        total_to_withdrawal,
        &trade.currency,
        TransactionCategory::PaymentFromTrade(trade.id),
    )?;

    // Update account balance and trade balance.
    let account_balance: AccountBalance =
        balance::calculate_account(database, &account, &trade.currency)?;
    let trade_balance: TradeBalance = balance::calculate_trade(database, trade)?;

    Ok((transaction, account_balance, trade_balance))
}
