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
        .checked_mul(trade.entry.quantity)
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
        .checked_mul(trade.entry.quantity)
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
        .checked_mul(trade.entry.quantity)
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
        .checked_mul(trade.entry.quantity)
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
        .checked_mul(trade.entry.quantity)
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

#[cfg(test)]
mod tests {
    use super::*;
    use db_sqlite::SqliteDatabase;
    use model::{Account, Environment, Order, Status, Trade};
    use rust_decimal_macros::dec;

    fn in_memory_database() -> SqliteDatabase {
        SqliteDatabase::new_in_memory()
    }

    fn submitted_trade_with_entry_fill(average_price: Decimal) -> Trade {
        Trade {
            status: Status::Submitted,
            entry: Order {
                average_filled_price: Some(average_price),
                ..Order::default()
            },
            balance: TradeBalance {
                funding: dec!(1_000),
                ..TradeBalance::default()
            },
            ..Trade::default()
        }
    }

    fn filled_trade_with_target_fill(average_price: Decimal) -> Trade {
        Trade {
            status: Status::Filled,
            target: Order {
                average_filled_price: Some(average_price),
                ..Order::default()
            },
            ..Trade::default()
        }
    }

    fn filled_trade_with_stop_fill(average_price: Decimal) -> Trade {
        Trade {
            status: Status::Filled,
            safety_stop: Order {
                average_filled_price: Some(average_price),
                ..Order::default()
            },
            ..Trade::default()
        }
    }

    fn create_account_with_balance(database: &mut SqliteDatabase) -> Account {
        let account = database
            .account_write()
            .create(
                "fee-test-account",
                "account with zero starting balance",
                Environment::Paper,
                dec!(0),
                dec!(0),
            )
            .expect("account should be created");
        database
            .account_balance_write()
            .create(&account, &Currency::USD)
            .expect("account balance should be created");
        account
    }

    fn create_account_without_balance(database: &mut SqliteDatabase) -> Account {
        database
            .account_write()
            .create(
                "transaction-test-account",
                "account without starting balance",
                Environment::Paper,
                dec!(0),
                dec!(0),
            )
            .expect("account should be created")
    }

    #[test]
    fn create_deposit_creates_missing_overview_then_updates_existing_overview() {
        let mut database = in_memory_database();
        let account = create_account_without_balance(&mut database);

        let (transaction, balance) = create(
            &mut database,
            &TransactionCategory::Deposit,
            dec!(1_000),
            &Currency::USD,
            account.id,
        )
        .expect("first deposit should create balance overview");
        assert_eq!(transaction.category, TransactionCategory::Deposit);
        assert_eq!(balance.total_balance, dec!(1_000));
        assert_eq!(balance.total_available, dec!(1_000));

        let (_transaction, balance) = create(
            &mut database,
            &TransactionCategory::Deposit,
            dec!(250),
            &Currency::USD,
            account.id,
        )
        .expect("second deposit should update existing balance overview");
        assert_eq!(balance.total_balance, dec!(1_250));
        assert_eq!(balance.total_available, dec!(1_250));
    }

    #[test]
    fn create_withdrawal_variants_update_balances_and_reject_manual_trade_categories() {
        let mut database = in_memory_database();
        let account = create_account_without_balance(&mut database);
        create(
            &mut database,
            &TransactionCategory::Deposit,
            dec!(1_000),
            &Currency::USD,
            account.id,
        )
        .expect("deposit should seed balance");

        let (_transaction, balance) = create(
            &mut database,
            &TransactionCategory::Withdrawal,
            dec!(100),
            &Currency::USD,
            account.id,
        )
        .expect("plain withdrawal should succeed");
        assert_eq!(balance.total_balance, dec!(900));
        assert_eq!(balance.total_available, dec!(900));

        let (_transaction, balance) = create(
            &mut database,
            &TransactionCategory::WithdrawalTax,
            dec!(25),
            &Currency::USD,
            account.id,
        )
        .expect("tax withdrawal should succeed");
        assert_eq!(balance.total_balance, dec!(875));
        assert_eq!(balance.total_available, dec!(900));

        let (_transaction, balance) = create(
            &mut database,
            &TransactionCategory::WithdrawalEarnings,
            dec!(50),
            &Currency::USD,
            account.id,
        )
        .expect("earnings withdrawal should succeed");
        assert_eq!(balance.total_balance, dec!(825));
        assert_eq!(balance.total_available, dec!(850));

        let error = create(
            &mut database,
            &TransactionCategory::OpenTrade(Uuid::new_v4()),
            dec!(10),
            &Currency::USD,
            account.id,
        )
        .expect_err("manual trade lifecycle transaction should be rejected");
        assert!(error
            .to_string()
            .contains("Manually creating transaction category"));
    }

    #[test]
    fn transfer_to_fill_trade_requires_entry_average_filled_price() {
        let mut database = in_memory_database();
        let trade = Trade {
            status: Status::Submitted,
            entry: Order {
                average_filled_price: None,
                ..Order::default()
            },
            ..Trade::default()
        };

        let error = transfer_to_fill_trade(&trade, &mut database)
            .expect_err("entry fills without average price must be rejected");

        assert_eq!(error.to_string(), "Entry order has no average filled price");
    }

    #[test]
    fn transfer_to_fill_trade_rejects_wrong_status_before_writing() {
        let mut database = in_memory_database();
        let trade = Trade {
            status: Status::New,
            ..submitted_trade_with_entry_fill(dec!(10))
        };

        let error = transfer_to_fill_trade(&trade, &mut database)
            .expect_err("unfunded trades must not create fill transactions");

        assert!(error.to_string().contains("Trade status is wrong"));
    }

    #[test]
    fn transfer_to_fill_trade_rejects_zero_fill_total() {
        let mut database = in_memory_database();
        let trade = submitted_trade_with_entry_fill(dec!(0));

        let error = transfer_to_fill_trade(&trade, &mut database)
            .expect_err("zero value fills must be rejected");

        assert!(error.to_string().contains("Filling must be positive"));
    }

    #[test]
    fn transfer_to_fill_trade_rejects_fill_total_overflow() {
        let mut database = in_memory_database();
        let trade = submitted_trade_with_entry_fill(Decimal::MAX);

        let error = transfer_to_fill_trade(&trade, &mut database)
            .expect_err("overflowing fill totals must be rejected before writing");

        assert!(error
            .to_string()
            .contains("Arithmetic overflow in multiplication"));
    }

    #[test]
    fn transfer_to_fill_trade_rejects_planned_entry_total_overflow() {
        let mut database = in_memory_database();
        let trade = Trade {
            entry: Order {
                average_filled_price: Some(dec!(1)),
                unit_price: Decimal::MAX,
                ..Order::default()
            },
            balance: TradeBalance {
                funding: dec!(1_000),
                ..TradeBalance::default()
            },
            status: Status::Submitted,
            ..Trade::default()
        };

        let error = transfer_to_fill_trade(&trade, &mut database)
            .expect_err("overflowing planned entry totals must be rejected before writing");

        assert!(error
            .to_string()
            .contains("Arithmetic overflow in multiplication"));
    }

    #[test]
    fn transfer_to_fill_trade_rejects_slippage_difference_overflow() {
        let mut database = in_memory_database();
        let trade = Trade {
            entry: Order {
                average_filled_price: Some(Decimal::MAX),
                unit_price: dec!(-1),
                quantity: 1.into(),
                ..Order::default()
            },
            balance: TradeBalance {
                funding: Decimal::MAX,
                ..TradeBalance::default()
            },
            status: Status::Submitted,
            ..Trade::default()
        };

        let error = transfer_to_fill_trade(&trade, &mut database)
            .expect_err("overflowing slippage differences must be rejected before writing");

        assert!(error
            .to_string()
            .contains("Arithmetic overflow in subtraction"));
    }

    #[test]
    fn transfer_to_close_target_requires_target_average_filled_price() {
        let mut database = in_memory_database();
        let trade = Trade {
            target: Order {
                average_filled_price: None,
                ..Order::default()
            },
            ..Trade::default()
        };

        let error = transfer_to_close_target(&trade, &mut database)
            .expect_err("target closes without average price must be rejected");

        assert_eq!(
            error.to_string(),
            "Target order has no average filled price"
        );
    }

    #[test]
    fn transfer_to_close_target_rejects_zero_close_total() {
        let mut database = in_memory_database();
        let trade = filled_trade_with_target_fill(dec!(0));

        let error = transfer_to_close_target(&trade, &mut database)
            .expect_err("zero target close value must be rejected");

        assert!(error.to_string().contains("Closing must be positive"));
    }

    #[test]
    fn transfer_to_close_target_rejects_close_total_overflow() {
        let mut database = in_memory_database();
        let trade = filled_trade_with_target_fill(Decimal::MAX);

        let error = transfer_to_close_target(&trade, &mut database)
            .expect_err("overflowing target close totals must be rejected before writing");

        assert!(error
            .to_string()
            .contains("Arithmetic overflow in multiplication"));
    }

    #[test]
    fn transfer_to_close_stop_requires_stop_average_filled_price() {
        let mut database = in_memory_database();
        let trade = Trade {
            safety_stop: Order {
                average_filled_price: None,
                ..Order::default()
            },
            ..Trade::default()
        };

        let error = transfer_to_close_stop(&trade, &mut database)
            .expect_err("stop closes without average price must be rejected");

        assert_eq!(
            error.to_string(),
            "Safety stop order has no average filled price"
        );
    }

    #[test]
    fn transfer_to_close_stop_rejects_zero_close_total() {
        let mut database = in_memory_database();
        let trade = filled_trade_with_stop_fill(dec!(0));

        let error = transfer_to_close_stop(&trade, &mut database)
            .expect_err("zero stop close value must be rejected");

        assert!(error.to_string().contains("Closing must be positive"));
    }

    #[test]
    fn transfer_to_close_stop_rejects_close_total_overflow() {
        let mut database = in_memory_database();
        let trade = filled_trade_with_stop_fill(Decimal::MAX);

        let error = transfer_to_close_stop(&trade, &mut database)
            .expect_err("overflowing stop close totals must be rejected before writing");

        assert!(error
            .to_string()
            .contains("Arithmetic overflow in multiplication"));
    }

    #[test]
    fn transfer_to_close_stop_rejects_planned_stop_total_overflow() {
        let mut database = in_memory_database();
        let trade = Trade {
            safety_stop: Order {
                average_filled_price: Some(dec!(1)),
                unit_price: Decimal::MAX,
                ..Order::default()
            },
            ..Trade::default()
        };

        let error = transfer_to_close_stop(&trade, &mut database)
            .expect_err("overflowing planned stop totals must be rejected before writing");

        assert!(error
            .to_string()
            .contains("Arithmetic overflow in multiplication"));
    }

    #[test]
    fn transfer_opening_fee_returns_balance_lookup_errors_before_writing() {
        let mut database = in_memory_database();
        let trade = Trade::default();

        let error = transfer_opening_fee(dec!(1), &trade, &mut database)
            .expect_err("fee transfer without an account balance must be rejected");

        assert!(error.to_string().contains("Record not found"));
    }

    #[test]
    fn transfer_closing_fee_rejects_non_positive_fee() {
        let mut database = in_memory_database();
        let account = create_account_with_balance(&mut database);
        let trade = Trade {
            account_id: account.id,
            ..Trade::default()
        };

        let error = transfer_closing_fee(dec!(0), &trade, &mut database)
            .expect_err("zero closing fees must be rejected");

        assert!(error.to_string().contains("Fee must be positive"));
    }
}
