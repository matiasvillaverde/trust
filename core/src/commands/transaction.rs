use model::{
    AccountBalance, Currency, DatabaseFactory, Trade, TradeBalance, Transaction,
    TransactionCategory,
};
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use std::error::Error;
use uuid::Uuid;

use crate::{
    calculators_trade::TradeCapitalRequired,
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
        TransactionCategory::Deposit => deposit(database, amount, currency, account_id),
        TransactionCategory::Withdrawal => withdraw(
            database,
            amount,
            currency,
            account_id,
            TransactionCategory::Withdrawal,
        ),
        TransactionCategory::WithdrawalTax => withdraw(
            database,
            amount,
            currency,
            account_id,
            TransactionCategory::WithdrawalTax,
        ),
        TransactionCategory::WithdrawalEarnings => withdraw(
            database,
            amount,
            currency,
            account_id,
            TransactionCategory::WithdrawalEarnings,
        ),
        default => {
            let message = format!("Manually creating transaction category {default:?} is not allowed. Only Withdrawals and deposits are allowed");
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
    match can_transfer_deposit(
        amount,
        currency,
        account_id,
        database.account_balance_read().as_mut(),
    ) {
        Ok(_) => {
            let transaction = database
                .transaction_write()
                .create_transaction_by_account_id(
                    account_id,
                    amount,
                    currency,
                    TransactionCategory::Deposit,
                )?;
            let updated_balance = balance::apply_account_projection_for_transaction_by_id(
                database,
                account_id,
                currency,
                TransactionCategory::Deposit,
                amount,
            )?;
            Ok((transaction, updated_balance))
        }
        Err(error) => {
            if error.code == TransactionValidationErrorCode::OverviewNotFound {
                let transaction = database
                    .transaction_write()
                    .create_transaction_by_account_id(
                        account_id,
                        amount,
                        currency,
                        TransactionCategory::Deposit,
                    )?;
                let account = database.account_read().id(account_id)?;
                database
                    .account_balance_write()
                    .create(&account, currency)?;
                let updated_balance = balance::apply_account_projection_for_transaction_by_id(
                    database,
                    account_id,
                    currency,
                    TransactionCategory::Deposit,
                    amount,
                )?;
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
    category: TransactionCategory,
) -> Result<(Transaction, AccountBalance), Box<dyn Error>> {
    // Validate that account has enough funds to withdraw
    transaction::can_transfer_withdraw(
        amount,
        currency,
        account_id,
        database.account_balance_read().as_mut(),
    )?;

    // Create transaction
    let transaction = database
        .transaction_write()
        .create_transaction_by_account_id(account_id, amount, currency, category)?;

    // Update account balance
    let updated_balance = balance::apply_account_projection_for_transaction_by_id(
        database, account_id, currency, category, amount,
    )?;

    Ok((transaction, updated_balance))
}

pub fn transfer_to_fund_trade(
    trade: &Trade,
    database: &mut dyn DatabaseFactory,
) -> Result<(Transaction, AccountBalance, TradeBalance), Box<dyn Error>> {
    // 1. Validate that trade can be fund
    crate::validators::funding::can_fund(trade, database)?;

    // Use the calculator to determine the required capital based on trade type.
    // For short trades, this uses the stop price (worst case) to ensure we have
    // enough capital even if the entry executes at a better price.
    let trade_total = TradeCapitalRequired::calculate(trade)?;

    let transaction = database
        .transaction_write()
        .create_transaction_by_account_id(
            trade.account_id,
            trade_total,
            &trade.currency,
            TransactionCategory::FundTrade(trade.id),
        )?;

    // 3. Update Account Overview and Trade Overview
    let account_balance = balance::apply_account_projection_with_in_trade_delta_by_id(
        database,
        trade.account_id,
        &trade.currency,
        TransactionCategory::FundTrade(trade.id),
        trade_total,
        trade_total,
    )?;
    let trade_balance: TradeBalance = balance::apply_trade_projection_for_transaction(
        database,
        trade,
        TransactionCategory::FundTrade(trade.id),
        trade_total,
    )?;

    Ok((transaction, account_balance, trade_balance))
}

pub fn transfer_to_fill_trade(
    trade: &Trade,
    database: &mut dyn DatabaseFactory,
) -> Result<(Transaction, TradeBalance), Box<dyn Error>> {
    // 1. Calculate the total amount of the trade
    let average_price = trade
        .entry
        .average_filled_price
        .ok_or("Entry order has no average filled price")?;
    let total = average_price
        .checked_mul(Decimal::from(trade.entry.quantity))
        .ok_or_else(|| {
            format!(
                "Arithmetic overflow in multiplication: {} * {}",
                average_price, trade.entry.quantity
            )
        })?;

    // 2. Validate that the trade has enough funds to fill the trade
    transaction::can_transfer_fill(trade, total)?;

    // 3. Create transaction
    // 4. If there is a difference between the unit_price and the average_filled_price
    // then we should create a transaction to transfer the difference to the account.
    let entry_total = trade
        .entry
        .unit_price
        .checked_mul(Decimal::from(trade.entry.quantity))
        .ok_or_else(|| {
            format!(
                "Arithmetic overflow in multiplication: {} * {}",
                trade.entry.unit_price, trade.entry.quantity
            )
        })?;

    let mut total_difference = total
        .checked_sub(entry_total)
        .ok_or_else(|| format!("Arithmetic overflow in subtraction: {total} - {entry_total}"))?;
    total_difference.set_sign_positive(true);

    let transaction = {
        let mut transaction_writer = database.transaction_write();
        let transaction = transaction_writer.create_transaction_by_account_id(
            trade.account_id,
            total,
            &trade.currency,
            TransactionCategory::OpenTrade(trade.id),
        )?;

        if total_difference > dec!(0) {
            transaction_writer.create_transaction_by_account_id(
                trade.account_id,
                total_difference,
                &trade.currency,
                TransactionCategory::PaymentFromTrade(trade.id),
            )?;
        }

        transaction
    };

    if total_difference > dec!(0) {
        let lifecycle_updates = [
            (TransactionCategory::OpenTrade(trade.id), total),
            (
                TransactionCategory::PaymentFromTrade(trade.id),
                total_difference,
            ),
        ];
        let _ = balance::apply_account_projection_batch_by_id(
            database,
            trade.account_id,
            &trade.currency,
            &lifecycle_updates,
            total,
        )?;
        let trade_balance =
            balance::apply_trade_projection_batch(database, trade, &lifecycle_updates)?;

        return Ok((transaction, trade_balance));
    }

    let lifecycle_updates = [(TransactionCategory::OpenTrade(trade.id), total)];
    let _ = balance::apply_account_projection_batch_by_id(
        database,
        trade.account_id,
        &trade.currency,
        &lifecycle_updates,
        total,
    )?;
    let trade_balance = balance::apply_trade_projection_batch(database, trade, &lifecycle_updates)?;

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
    let transaction = database
        .transaction_write()
        .create_transaction_by_account_id(
            trade.account_id,
            fee,
            &trade.currency,
            TransactionCategory::FeeOpen(trade.id),
        )?;

    // 3. Update account balance
    let updated_balance = balance::apply_account_projection_for_transaction_by_id(
        database,
        trade.account_id,
        &trade.currency,
        TransactionCategory::FeeOpen(trade.id),
        fee,
    )?;
    let _ = balance::apply_trade_projection_for_transaction(
        database,
        trade,
        TransactionCategory::FeeOpen(trade.id),
        fee,
    )?;

    Ok((transaction, updated_balance))
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

    let transaction = database
        .transaction_write()
        .create_transaction_by_account_id(
            trade.account_id,
            fee,
            &trade.currency,
            TransactionCategory::FeeClose(trade.id),
        )?;

    // Update account balance
    let updated_balance = balance::apply_account_projection_for_transaction_by_id(
        database,
        trade.account_id,
        &trade.currency,
        TransactionCategory::FeeClose(trade.id),
        fee,
    )?;
    let _ = balance::apply_trade_projection_for_transaction(
        database,
        trade,
        TransactionCategory::FeeClose(trade.id),
        fee,
    )?;

    Ok((transaction, updated_balance))
}

pub fn transfer_to_close_target(
    trade: &Trade,
    database: &mut dyn DatabaseFactory,
) -> Result<(Transaction, TradeBalance), Box<dyn Error>> {
    let average_price = trade
        .target
        .average_filled_price
        .ok_or("Target order has no average filled price")?;
    let total = average_price
        .checked_mul(Decimal::from(trade.entry.quantity))
        .ok_or_else(|| {
            format!(
                "Arithmetic overflow in multiplication: {} * {}",
                average_price, trade.entry.quantity
            )
        })?;

    // 1. Validate that the closing is possible
    transaction::can_transfer_close(total)?;

    // 2. Create transaction
    let transaction = database
        .transaction_write()
        .create_transaction_by_account_id(
            trade.account_id,
            total,
            &trade.currency,
            TransactionCategory::CloseTarget(trade.id),
        )?;

    // 3. Update trade balance and account balance
    let lifecycle_updates = [(TransactionCategory::CloseTarget(trade.id), total)];
    let trade_balance = balance::apply_trade_projection_batch(database, trade, &lifecycle_updates)?;
    let _ = balance::apply_account_projection_batch_by_id(
        database,
        trade.account_id,
        &trade.currency,
        &lifecycle_updates,
        Decimal::ZERO,
    )?;

    Ok((transaction, trade_balance))
}

pub fn transfer_to_close_stop(
    trade: &Trade,
    database: &mut dyn DatabaseFactory,
) -> Result<(Transaction, TradeBalance), Box<dyn Error>> {
    // 1. Calculate the total amount of the trade
    let average_price = trade
        .safety_stop
        .average_filled_price
        .ok_or("Safety stop order has no average filled price")?;
    let total = average_price
        .checked_mul(Decimal::from(trade.entry.quantity))
        .ok_or_else(|| {
            format!(
                "Arithmetic overflow in multiplication: {} * {}",
                average_price, trade.entry.quantity
            )
        })?;

    // 2. Validate that the closing is possible
    transaction::can_transfer_close(total)?;

    // 3. If the stop was lower than the planned price, then we should create a transaction
    // with category slippage. For more information see: https://www.investopedia.com/terms/s/slippage.asp
    let planned_total = trade
        .safety_stop
        .unit_price
        .checked_mul(Decimal::from(trade.entry.quantity))
        .ok_or_else(|| {
            format!(
                "Arithmetic overflow in multiplication: {} * {}",
                trade.safety_stop.unit_price, trade.entry.quantity
            )
        })?;

    let category = if total > planned_total {
        TransactionCategory::CloseSafetyStopSlippage(trade.id)
    } else {
        TransactionCategory::CloseSafetyStop(trade.id)
    };

    // 4. Create transaction
    let transaction = database
        .transaction_write()
        .create_transaction_by_account_id(trade.account_id, total, &trade.currency, category)?;

    // 5. Update trade balance and account balance
    let lifecycle_updates = [(category, total)];
    let trade_balance = balance::apply_trade_projection_batch(database, trade, &lifecycle_updates)?;
    let _ = balance::apply_account_projection_batch_by_id(
        database,
        trade.account_id,
        &trade.currency,
        &lifecycle_updates,
        Decimal::ZERO,
    )?;

    Ok((transaction, trade_balance))
}

pub fn transfer_to_account_from(
    trade: &Trade,
    database: &mut dyn DatabaseFactory,
) -> Result<(Transaction, AccountBalance, TradeBalance), Box<dyn Error>> {
    // Create transaction
    let trade_balance = database.trade_read().read_trade_balance(trade.balance.id)?;
    let total_to_withdrawal = trade_balance.capital_out_market;

    let transaction = database
        .transaction_write()
        .create_transaction_by_account_id(
            trade.account_id,
            total_to_withdrawal,
            &trade.currency,
            TransactionCategory::PaymentFromTrade(trade.id),
        )?;

    // Update account balance and trade balance.
    let lifecycle_updates = [(
        TransactionCategory::PaymentFromTrade(trade.id),
        total_to_withdrawal,
    )];
    let account_balance: AccountBalance = balance::apply_account_projection_batch_by_id(
        database,
        trade.account_id,
        &trade.currency,
        &lifecycle_updates,
        Decimal::ZERO,
    )?;
    let trade_balance: TradeBalance = balance::apply_trade_projection_batch_with_current_balance(
        database,
        trade,
        trade_balance,
        &lifecycle_updates,
    )?;

    Ok((transaction, account_balance, trade_balance))
}
