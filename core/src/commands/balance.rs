use model::{
    Account, AccountBalance, Currency, DatabaseFactory, Status, Trade, TradeBalance,
    TransactionCategory,
};
use rust_decimal::Decimal;
use std::error::Error;
use uuid::Uuid;

use crate::{
    calculators_account::{
        AccountCapitalAvailable, AccountCapitalBalance, AccountCapitalInApprovedTrades,
        AccountCapitalTaxable,
    },
    calculators_trade::{TradeCapitalFunded, TradeCapitalInMarket},
    calculators_trade::{TradeCapitalOutOfMarket, TradeCapitalTaxable, TradePerformance},
};

fn checked_add(
    current: Decimal,
    amount: Decimal,
    context: &str,
) -> Result<Decimal, Box<dyn Error>> {
    current.checked_add(amount).ok_or_else(|| {
        format!("Arithmetic overflow in addition ({context}): {current} + {amount}").into()
    })
}

fn checked_sub(
    current: Decimal,
    amount: Decimal,
    context: &str,
) -> Result<Decimal, Box<dyn Error>> {
    current.checked_sub(amount).ok_or_else(|| {
        format!("Arithmetic overflow in subtraction ({context}): {current} - {amount}").into()
    })
}

fn reduce_account_total_balance(
    current: Decimal,
    category: TransactionCategory,
    amount: Decimal,
) -> Result<Decimal, Box<dyn Error>> {
    match category {
        TransactionCategory::Deposit
        | TransactionCategory::CloseTarget(_)
        | TransactionCategory::CloseSafetyStop(_)
        | TransactionCategory::CloseSafetyStopSlippage(_) => {
            checked_add(current, amount, "account total_balance increase")
        }
        TransactionCategory::Withdrawal
        | TransactionCategory::OpenTrade(_)
        | TransactionCategory::FeeOpen(_)
        | TransactionCategory::FeeClose(_)
        | TransactionCategory::WithdrawalTax
        | TransactionCategory::WithdrawalEarnings => {
            checked_sub(current, amount, "account total_balance decrease")
        }
        TransactionCategory::FundTrade(_)
        | TransactionCategory::PaymentFromTrade(_)
        | TransactionCategory::PaymentTax(_)
        | TransactionCategory::PaymentEarnings(_) => Ok(current),
    }
}

fn reduce_account_total_available(
    current: Decimal,
    category: TransactionCategory,
    amount: Decimal,
) -> Result<Decimal, Box<dyn Error>> {
    match category {
        TransactionCategory::Deposit | TransactionCategory::PaymentFromTrade(_) => {
            checked_add(current, amount, "account total_available increase")
        }
        TransactionCategory::Withdrawal
        | TransactionCategory::FundTrade(_)
        | TransactionCategory::FeeOpen(_)
        | TransactionCategory::FeeClose(_)
        | TransactionCategory::WithdrawalEarnings => {
            checked_sub(current, amount, "account total_available decrease")
        }
        TransactionCategory::OpenTrade(_)
        | TransactionCategory::CloseTarget(_)
        | TransactionCategory::CloseSafetyStop(_)
        | TransactionCategory::CloseSafetyStopSlippage(_)
        | TransactionCategory::PaymentTax(_)
        | TransactionCategory::WithdrawalTax
        | TransactionCategory::PaymentEarnings(_) => Ok(current),
    }
}

fn reduce_account_taxed(
    current: Decimal,
    category: TransactionCategory,
    amount: Decimal,
) -> Result<Decimal, Box<dyn Error>> {
    match category {
        TransactionCategory::PaymentTax(_) => {
            checked_add(current, amount, "account taxed increase")
        }
        TransactionCategory::WithdrawalTax => {
            checked_sub(current, amount, "account taxed decrease")
        }
        _ => Ok(current),
    }
}

fn reduce_account_projection(
    balance: AccountBalance,
    category: TransactionCategory,
    amount: Decimal,
) -> Result<AccountBalance, Box<dyn Error>> {
    let next_total_balance = reduce_account_total_balance(balance.total_balance, category, amount)?;
    let next_total_available =
        reduce_account_total_available(balance.total_available, category, amount)?;
    let next_taxed = reduce_account_taxed(balance.taxed, category, amount)?;

    if next_total_available.is_sign_negative() {
        return Err(format!(
            "account projection invariant failed: total_available is negative ({next_total_available})"
        )
        .into());
    }

    Ok(AccountBalance {
        total_balance: next_total_balance,
        total_available: next_total_available,
        taxed: next_taxed,
        ..balance
    })
}

fn reduce_trade_funding(
    current: Decimal,
    category: TransactionCategory,
    amount: Decimal,
) -> Result<Decimal, Box<dyn Error>> {
    match category {
        TransactionCategory::FundTrade(_) => checked_add(current, amount, "trade funding increase"),
        _ => Ok(current),
    }
}

fn reduce_trade_capital_in_market(
    current: Decimal,
    category: TransactionCategory,
    amount: Decimal,
) -> Result<Decimal, Box<dyn Error>> {
    match category {
        TransactionCategory::OpenTrade(_) => {
            checked_add(current, amount, "trade capital_in_market increase")
        }
        TransactionCategory::CloseTarget(_)
        | TransactionCategory::CloseSafetyStop(_)
        | TransactionCategory::CloseSafetyStopSlippage(_) => Ok(Decimal::ZERO),
        _ => Ok(current),
    }
}

fn reduce_trade_capital_out_market(
    current: Decimal,
    category: TransactionCategory,
    amount: Decimal,
) -> Result<Decimal, Box<dyn Error>> {
    match category {
        TransactionCategory::FundTrade(_)
        | TransactionCategory::CloseTarget(_)
        | TransactionCategory::CloseSafetyStop(_)
        | TransactionCategory::CloseSafetyStopSlippage(_) => {
            checked_add(current, amount, "trade capital_out_market increase")
        }
        TransactionCategory::PaymentFromTrade(_) | TransactionCategory::OpenTrade(_) => {
            checked_sub(current, amount, "trade capital_out_market decrease")
        }
        _ => Ok(current),
    }
}

fn reduce_trade_taxed(
    current: Decimal,
    category: TransactionCategory,
    amount: Decimal,
) -> Result<Decimal, Box<dyn Error>> {
    match category {
        TransactionCategory::PaymentTax(_) => checked_add(current, amount, "trade taxed increase"),
        _ => Ok(current),
    }
}

fn reduce_trade_total_performance(
    current: Decimal,
    category: TransactionCategory,
    amount: Decimal,
) -> Result<Decimal, Box<dyn Error>> {
    match category {
        TransactionCategory::CloseTarget(_)
        | TransactionCategory::CloseSafetyStop(_)
        | TransactionCategory::CloseSafetyStopSlippage(_) => {
            checked_add(current, amount, "trade total_performance increase")
        }
        TransactionCategory::OpenTrade(_)
        | TransactionCategory::FeeOpen(_)
        | TransactionCategory::FeeClose(_)
        | TransactionCategory::PaymentTax(_) => {
            checked_sub(current, amount, "trade total_performance decrease")
        }
        _ => Ok(current),
    }
}

fn reduce_trade_projection(
    balance: TradeBalance,
    category: TransactionCategory,
    amount: Decimal,
) -> Result<TradeBalance, Box<dyn Error>> {
    let next_funding = reduce_trade_funding(balance.funding, category, amount)?;
    let next_capital_in_market =
        reduce_trade_capital_in_market(balance.capital_in_market, category, amount)?;
    let next_capital_out_market =
        reduce_trade_capital_out_market(balance.capital_out_market, category, amount)?;
    let next_taxed = reduce_trade_taxed(balance.taxed, category, amount)?;
    let next_total_performance =
        reduce_trade_total_performance(balance.total_performance, category, amount)?;

    if next_capital_in_market.is_sign_negative() {
        return Err(format!(
            "trade projection invariant failed: capital_in_market is negative ({next_capital_in_market})"
        )
        .into());
    }

    Ok(TradeBalance {
        funding: next_funding,
        capital_in_market: next_capital_in_market,
        capital_out_market: next_capital_out_market,
        taxed: next_taxed,
        total_performance: next_total_performance,
        ..balance
    })
}

fn accumulate_account_projection(
    current: AccountBalance,
    updates: &[(TransactionCategory, Decimal)],
) -> Result<AccountBalance, Box<dyn Error>> {
    updates
        .iter()
        .try_fold(current, |balance, (category, amount)| {
            reduce_account_projection(balance, *category, *amount)
        })
}

fn accumulate_trade_projection(
    current: TradeBalance,
    updates: &[(TransactionCategory, Decimal)],
) -> Result<TradeBalance, Box<dyn Error>> {
    updates
        .iter()
        .try_fold(current, |balance, (category, amount)| {
            reduce_trade_projection(balance, *category, *amount)
        })
}

fn checked_neg(amount: Decimal, context: &str) -> Result<Decimal, Box<dyn Error>> {
    Decimal::ZERO
        .checked_sub(amount)
        .ok_or_else(|| format!("Arithmetic overflow in negation ({context}): 0 - {amount}").into())
}

fn in_trade_delta_for_status_transition(
    trade: &Trade,
    new_status: Status,
) -> Result<Option<Decimal>, Box<dyn Error>> {
    match (trade.status, new_status) {
        (Status::Funded, Status::Submitted) | (Status::Funded, Status::Canceled) => Ok(Some(
            checked_neg(trade.balance.funding, "status transition funded")?,
        )),
        (Status::Filled, Status::ClosedTarget)
        | (Status::Filled, Status::ClosedStopLoss)
        | (Status::Filled, Status::Expired)
        | (Status::Filled, Status::Rejected)
        | (Status::Canceled, Status::ClosedTarget)
        | (Status::Canceled, Status::ClosedStopLoss)
        | (Status::Canceled, Status::Expired)
        | (Status::Canceled, Status::Rejected) => Ok(Some(checked_neg(
            trade.balance.capital_in_market,
            "status transition filled",
        )?)),
        _ => Ok(None),
    }
}

fn should_verify_projection_recalc() -> bool {
    std::env::var_os("TRUST_VERIFY_PROJECTION_RECALC").is_some()
}

fn verify_account_projection_recalc(
    database: &mut dyn DatabaseFactory,
    account_id: Uuid,
    currency: &Currency,
    updated: &AccountBalance,
    context: &str,
) -> Result<(), Box<dyn Error>> {
    if !should_verify_projection_recalc() {
        return Ok(());
    }

    let recalculated = recompute_account_projection_by_id(database, account_id, currency)?;
    if updated.total_balance != recalculated.total_balance
        || updated.total_in_trade != recalculated.total_in_trade
        || updated.total_available != recalculated.total_available
        || updated.taxed != recalculated.taxed
    {
        return Err(format!(
            "account projection mismatch detected during verification ({context}): \
             updated(total_balance={}, total_in_trade={}, total_available={}, taxed={}) \
             recalculated(total_balance={}, total_in_trade={}, total_available={}, taxed={})",
            updated.total_balance,
            updated.total_in_trade,
            updated.total_available,
            updated.taxed,
            recalculated.total_balance,
            recalculated.total_in_trade,
            recalculated.total_available,
            recalculated.taxed
        )
        .into());
    }

    Ok(())
}

fn verify_trade_projection_recalc(
    database: &mut dyn DatabaseFactory,
    trade: &Trade,
    updated: &TradeBalance,
    context: &str,
) -> Result<(), Box<dyn Error>> {
    if !should_verify_projection_recalc() {
        return Ok(());
    }

    let recalculated = recompute_trade_projection(database, trade)?;
    if updated.funding != recalculated.funding
        || updated.capital_in_market != recalculated.capital_in_market
        || updated.capital_out_market != recalculated.capital_out_market
        || updated.taxed != recalculated.taxed
        || updated.total_performance != recalculated.total_performance
    {
        return Err(format!(
            "trade projection mismatch detected during verification ({context}): \
             updated(funding={}, capital_in_market={}, capital_out_market={}, taxed={}, total_performance={}) \
             recalculated(funding={}, capital_in_market={}, capital_out_market={}, taxed={}, total_performance={})",
            updated.funding,
            updated.capital_in_market,
            updated.capital_out_market,
            updated.taxed,
            updated.total_performance,
            recalculated.funding,
            recalculated.capital_in_market,
            recalculated.capital_out_market,
            recalculated.taxed,
            recalculated.total_performance
        )
        .into());
    }

    Ok(())
}

fn recompute_account_projection(
    database: &mut dyn DatabaseFactory,
    account: &Account,
    currency: &Currency,
) -> Result<AccountBalance, Box<dyn Error>> {
    recompute_account_projection_by_id(database, account.id, currency)
}

fn recompute_account_projection_by_id(
    database: &mut dyn DatabaseFactory,
    account_id: Uuid,
    currency: &Currency,
) -> Result<AccountBalance, Box<dyn Error>> {
    let mut transaction_reader = database.transaction_read();
    let total_available =
        AccountCapitalAvailable::calculate(account_id, currency, transaction_reader.as_mut())?;
    let total_in_trade = AccountCapitalInApprovedTrades::calculate(
        account_id,
        currency,
        transaction_reader.as_mut(),
    )?;
    let taxed =
        AccountCapitalTaxable::calculate(account_id, currency, transaction_reader.as_mut())?;
    let total_balance =
        AccountCapitalBalance::calculate(account_id, currency, transaction_reader.as_mut())?;

    let current = database
        .account_balance_read()
        .for_currency(account_id, currency)?;
    Ok(AccountBalance {
        total_balance,
        total_in_trade,
        total_available,
        taxed,
        ..current
    })
}

fn recompute_trade_projection(
    database: &mut dyn DatabaseFactory,
    trade: &Trade,
) -> Result<TradeBalance, Box<dyn Error>> {
    let current_trade = database.trade_read().read_trade(trade.id)?;
    let mut transaction_reader = database.transaction_read();
    let funding = TradeCapitalFunded::calculate(trade.id, transaction_reader.as_mut())?;
    let capital_in_market = TradeCapitalInMarket::calculate(trade.id, transaction_reader.as_mut())?;
    let capital_out_market =
        TradeCapitalOutOfMarket::calculate(trade.id, transaction_reader.as_mut())?;
    let taxed = TradeCapitalTaxable::calculate(trade.id, transaction_reader.as_mut())?;
    let total_performance = TradePerformance::calculate(trade.id, transaction_reader.as_mut())?;

    Ok(TradeBalance {
        funding,
        capital_in_market,
        capital_out_market,
        taxed,
        total_performance,
        ..current_trade.balance
    })
}

pub fn apply_account_projection_for_transaction_by_id(
    database: &mut dyn DatabaseFactory,
    account_id: Uuid,
    currency: &Currency,
    category: TransactionCategory,
    amount: Decimal,
) -> Result<AccountBalance, Box<dyn Error>> {
    let current_balance = database
        .account_balance_read()
        .for_currency(account_id, currency)?;
    let next_balance = accumulate_account_projection(current_balance, &[(category, amount)])?;
    let updated = database.account_balance_write().update(
        &current_balance,
        next_balance.total_balance,
        next_balance.total_in_trade,
        next_balance.total_available,
        next_balance.taxed,
    )?;

    verify_account_projection_recalc(
        database,
        account_id,
        currency,
        &updated,
        &format!("category={category:?}, amount={amount}"),
    )?;

    Ok(updated)
}

pub fn apply_account_projection_with_in_trade_delta_by_id(
    database: &mut dyn DatabaseFactory,
    account_id: Uuid,
    currency: &Currency,
    category: TransactionCategory,
    amount: Decimal,
    in_trade_delta: Decimal,
) -> Result<AccountBalance, Box<dyn Error>> {
    let current_balance = database
        .account_balance_read()
        .for_currency(account_id, currency)?;
    let reduced_balance = accumulate_account_projection(current_balance, &[(category, amount)])?;
    let next_in_trade = checked_add(
        current_balance.total_in_trade,
        in_trade_delta,
        "account total_in_trade combined transaction update",
    )?;

    if next_in_trade.is_sign_negative() {
        return Err(format!(
            "account projection invariant failed: total_in_trade is negative ({next_in_trade})"
        )
        .into());
    }

    let updated = database.account_balance_write().update(
        &current_balance,
        reduced_balance.total_balance,
        next_in_trade,
        reduced_balance.total_available,
        reduced_balance.taxed,
    )?;

    verify_account_projection_recalc(
        database,
        account_id,
        currency,
        &updated,
        &format!(
            "combined updates: category={category:?}, amount={amount}, in_trade_delta={in_trade_delta}"
        ),
    )?;

    Ok(updated)
}

pub fn apply_account_projection_batch_by_id(
    database: &mut dyn DatabaseFactory,
    account_id: Uuid,
    currency: &Currency,
    updates: &[(TransactionCategory, Decimal)],
    in_trade_delta: Decimal,
) -> Result<AccountBalance, Box<dyn Error>> {
    let current_balance = database
        .account_balance_read()
        .for_currency(account_id, currency)?;

    if updates.is_empty() && in_trade_delta == Decimal::ZERO {
        return Ok(current_balance);
    }

    let reduced_balance = accumulate_account_projection(current_balance, updates)?;

    let next_in_trade = checked_add(
        current_balance.total_in_trade,
        in_trade_delta,
        "account total_in_trade batched transaction update",
    )?;

    if next_in_trade.is_sign_negative() {
        return Err(format!(
            "account projection invariant failed: total_in_trade is negative ({next_in_trade})"
        )
        .into());
    }

    let updated = database.account_balance_write().update(
        &current_balance,
        reduced_balance.total_balance,
        next_in_trade,
        reduced_balance.total_available,
        reduced_balance.taxed,
    )?;

    verify_account_projection_recalc(
        database,
        account_id,
        currency,
        &updated,
        &format!("batched updates={updates:?}, in_trade_delta={in_trade_delta}"),
    )?;

    Ok(updated)
}

pub fn apply_account_in_trade_delta_by_id(
    database: &mut dyn DatabaseFactory,
    account_id: Uuid,
    currency: &Currency,
    delta: Decimal,
) -> Result<AccountBalance, Box<dyn Error>> {
    let current_balance = database
        .account_balance_read()
        .for_currency(account_id, currency)?;
    let next_in_trade = checked_add(
        current_balance.total_in_trade,
        delta,
        "account total_in_trade status transition",
    )?;

    if next_in_trade.is_sign_negative() {
        return Err(format!(
            "account projection invariant failed: total_in_trade is negative ({next_in_trade})"
        )
        .into());
    }

    let updated = database.account_balance_write().update(
        &current_balance,
        current_balance.total_balance,
        next_in_trade,
        current_balance.total_available,
        current_balance.taxed,
    )?;

    verify_account_projection_recalc(
        database,
        account_id,
        currency,
        &updated,
        &format!("in-trade delta={delta}"),
    )?;

    Ok(updated)
}

pub fn apply_account_projection_for_trade_status_transition(
    database: &mut dyn DatabaseFactory,
    trade: &Trade,
    new_status: Status,
) -> Result<Option<AccountBalance>, Box<dyn Error>> {
    match in_trade_delta_for_status_transition(trade, new_status)? {
        Some(delta) if delta != Decimal::ZERO => {
            let updated = apply_account_in_trade_delta_by_id(
                database,
                trade.account_id,
                &trade.currency,
                delta,
            )?;
            Ok(Some(updated))
        }
        _ => Ok(None),
    }
}

pub fn apply_trade_projection_for_transaction(
    database: &mut dyn DatabaseFactory,
    trade: &Trade,
    category: TransactionCategory,
    amount: Decimal,
) -> Result<TradeBalance, Box<dyn Error>> {
    let current_balance = database.trade_read().read_trade_balance(trade.balance.id)?;
    apply_trade_projection_for_transaction_with_current_balance(
        database,
        trade,
        current_balance,
        category,
        amount,
    )
}

pub fn apply_trade_projection_for_transaction_with_current_balance(
    database: &mut dyn DatabaseFactory,
    trade: &Trade,
    current_balance: TradeBalance,
    category: TransactionCategory,
    amount: Decimal,
) -> Result<TradeBalance, Box<dyn Error>> {
    let next_balance = accumulate_trade_projection(current_balance, &[(category, amount)])?;

    let updated = database.trade_balance_write().update_trade_balance(
        trade,
        next_balance.funding,
        next_balance.capital_in_market,
        next_balance.capital_out_market,
        next_balance.taxed,
        next_balance.total_performance,
    )?;

    verify_trade_projection_recalc(
        database,
        trade,
        &updated,
        &format!("category={category:?}, amount={amount}"),
    )?;

    Ok(updated)
}

pub fn apply_trade_projection_batch(
    database: &mut dyn DatabaseFactory,
    trade: &Trade,
    updates: &[(TransactionCategory, Decimal)],
) -> Result<TradeBalance, Box<dyn Error>> {
    let current_balance = database.trade_read().read_trade_balance(trade.balance.id)?;
    apply_trade_projection_batch_with_current_balance(database, trade, current_balance, updates)
}

pub fn apply_trade_projection_batch_with_current_balance(
    database: &mut dyn DatabaseFactory,
    trade: &Trade,
    current_balance: TradeBalance,
    updates: &[(TransactionCategory, Decimal)],
) -> Result<TradeBalance, Box<dyn Error>> {
    if updates.is_empty() {
        return Ok(current_balance);
    }

    let reduced_balance = accumulate_trade_projection(current_balance, updates)?;

    let updated = database.trade_balance_write().update_trade_balance(
        trade,
        reduced_balance.funding,
        reduced_balance.capital_in_market,
        reduced_balance.capital_out_market,
        reduced_balance.taxed,
        reduced_balance.total_performance,
    )?;

    verify_trade_projection_recalc(
        database,
        trade,
        &updated,
        &format!("batched updates={updates:?}"),
    )?;

    Ok(updated)
}

#[allow(dead_code)]
pub fn calculate_account(
    database: &mut dyn DatabaseFactory,
    account: &Account,
    currency: &Currency,
) -> Result<AccountBalance, Box<dyn Error>> {
    let balance = recompute_account_projection(database, account, currency)?;
    let current_balance = database
        .account_balance_read()
        .for_currency(account.id, currency)?;

    database.account_balance_write().update(
        &current_balance,
        balance.total_balance,
        balance.total_in_trade,
        balance.total_available,
        balance.taxed,
    )
}

#[allow(dead_code)]
pub fn calculate_trade(
    database: &mut dyn DatabaseFactory,
    trade: &Trade,
) -> Result<TradeBalance, Box<dyn Error>> {
    let balance = recompute_trade_projection(database, trade)?;
    let latest_trade = database.trade_read().read_trade(trade.id)?;
    database.trade_balance_write().update_trade_balance(
        &latest_trade,
        balance.funding,
        balance.capital_in_market,
        balance.capital_out_market,
        balance.taxed,
        balance.total_performance,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use db_sqlite::SqliteDatabase;
    use model::{
        DraftTrade, Environment, Order, OrderAction, OrderCategory, TradeCategory, TradingVehicle,
        TradingVehicleCategory, Transaction,
    };
    use rust_decimal_macros::dec;
    use uuid::Uuid;

    fn sample_account_balance() -> AccountBalance {
        AccountBalance {
            id: Uuid::new_v4(),
            account_id: Uuid::new_v4(),
            currency: Currency::USD,
            total_balance: dec!(5000),
            total_available: dec!(4000),
            total_in_trade: dec!(1000),
            taxed: dec!(0),
            ..AccountBalance::default()
        }
    }

    fn sample_trade_balance() -> TradeBalance {
        TradeBalance {
            id: Uuid::new_v4(),
            currency: Currency::USD,
            funding: dec!(1000),
            capital_in_market: dec!(0),
            capital_out_market: dec!(1000),
            taxed: dec!(0),
            total_performance: dec!(0),
            ..TradeBalance::default()
        }
    }

    fn sample_trade(status: Status, funding: Decimal, in_market: Decimal) -> Trade {
        let now = Utc::now().naive_utc();
        Trade {
            id: Uuid::new_v4(),
            created_at: now,
            updated_at: now,
            deleted_at: None,
            trading_vehicle: TradingVehicle::default(),
            category: TradeCategory::Long,
            status,
            currency: Currency::USD,
            settlement_date: None,
            safety_stop: model::Order::default(),
            entry: model::Order::default(),
            target: model::Order::default(),
            account_id: Uuid::new_v4(),
            balance: TradeBalance {
                funding,
                capital_in_market: in_market,
                ..TradeBalance::default()
            },
            thesis: None,
            sector: None,
            asset_class: None,
            context: None,
        }
    }

    fn create_sqlite_account(database: &mut SqliteDatabase, name: &str) -> Account {
        database
            .account_write()
            .create(name, name, Environment::Paper, dec!(0), dec!(0))
            .expect("account should be created")
    }

    fn create_sqlite_account_balance(
        database: &mut SqliteDatabase,
        account: &Account,
    ) -> AccountBalance {
        database
            .account_balance_write()
            .create(account, &Currency::USD)
            .expect("account balance should be created")
    }

    fn create_sqlite_vehicle(database: &mut SqliteDatabase, symbol: &str) -> TradingVehicle {
        database
            .trading_vehicle_write()
            .create_trading_vehicle(
                symbol,
                Some(symbol),
                &TradingVehicleCategory::Stock,
                "alpaca",
            )
            .expect("trading vehicle should be created")
    }

    fn create_sqlite_order(
        database: &mut SqliteDatabase,
        vehicle: &TradingVehicle,
        action: OrderAction,
        category: OrderCategory,
        price: Decimal,
    ) -> Order {
        database
            .order_write()
            .create(vehicle, dec!(10), price, &Currency::USD, &action, &category)
            .expect("order should be created")
    }

    fn create_sqlite_trade(
        database: &mut SqliteDatabase,
        account: &Account,
        symbol: &str,
    ) -> Trade {
        let vehicle = create_sqlite_vehicle(database, symbol);
        let stop = create_sqlite_order(
            database,
            &vehicle,
            OrderAction::Sell,
            OrderCategory::Stop,
            dec!(90),
        );
        let entry = create_sqlite_order(
            database,
            &vehicle,
            OrderAction::Buy,
            OrderCategory::Limit,
            dec!(100),
        );
        let target = create_sqlite_order(
            database,
            &vehicle,
            OrderAction::Sell,
            OrderCategory::Limit,
            dec!(120),
        );

        let draft = DraftTrade {
            account: account.clone(),
            trading_vehicle: vehicle,
            quantity: 10.into(),
            currency: Currency::USD,
            category: TradeCategory::Long,
            thesis: Some("balance recalculation test".to_string()),
            sector: Some("technology".to_string()),
            asset_class: Some("equity".to_string()),
            context: Some("unit test".to_string()),
        };

        database
            .trade_write()
            .create_trade(draft, &stop, &entry, &target)
            .expect("trade should be created")
    }

    fn create_sqlite_transaction(
        database: &mut SqliteDatabase,
        account: &Account,
        category: TransactionCategory,
        amount: Decimal,
    ) -> Transaction {
        database
            .transaction_write()
            .create_transaction(account, amount, &Currency::USD, category)
            .expect("transaction should be created")
    }

    #[test]
    fn reduce_account_projection_updates_expected_fields() {
        let current = sample_account_balance();

        let after_fee = reduce_account_projection(
            current,
            TransactionCategory::FeeOpen(Uuid::new_v4()),
            dec!(10),
        )
        .expect("reduce account fee");
        assert_eq!(after_fee.total_balance, dec!(4990));
        assert_eq!(after_fee.total_available, dec!(3990));
        assert_eq!(after_fee.total_in_trade, dec!(1000));

        let after_open = reduce_account_projection(
            after_fee,
            TransactionCategory::OpenTrade(Uuid::new_v4()),
            dec!(390),
        )
        .expect("reduce account open");
        assert_eq!(after_open.total_balance, dec!(4600));
        assert_eq!(after_open.total_available, dec!(3990));
        assert_eq!(after_open.total_in_trade, dec!(1000));

        let after_tax = reduce_account_projection(
            after_open,
            TransactionCategory::PaymentTax(Uuid::new_v4()),
            dec!(80),
        )
        .expect("reduce account tax");
        assert_eq!(after_tax.taxed, dec!(80));
    }

    #[test]
    fn checked_decimal_helpers_report_overflow() {
        let add_error = checked_add(Decimal::MAX, Decimal::ONE, "test add")
            .expect_err("overflowing additions must be rejected");
        assert_eq!(
            add_error.to_string(),
            format!(
                "Arithmetic overflow in addition (test add): {} + {}",
                Decimal::MAX,
                Decimal::ONE
            )
        );

        let sub_error = checked_sub(Decimal::MIN, Decimal::ONE, "test sub")
            .expect_err("overflowing subtractions must be rejected");
        assert_eq!(
            sub_error.to_string(),
            format!(
                "Arithmetic overflow in subtraction (test sub): {} - {}",
                Decimal::MIN,
                Decimal::ONE
            )
        );

        assert_eq!(
            checked_neg(Decimal::MIN, "test neg").expect("Decimal::MIN negation should fit"),
            Decimal::MAX
        );
    }

    #[test]
    fn reduce_account_projection_rejects_negative_available_cash() {
        let current = AccountBalance {
            total_available: Decimal::ZERO,
            ..sample_account_balance()
        };

        let error = reduce_account_projection(current, TransactionCategory::Withdrawal, dec!(1))
            .expect_err("account available cash must not project negative");

        assert_eq!(
            error.to_string(),
            "account projection invariant failed: total_available is negative (-1)"
        );
    }

    #[test]
    fn reduce_trade_projection_matches_trade_lifecycle() {
        let current = sample_trade_balance();

        let after_open = reduce_trade_projection(
            current,
            TransactionCategory::OpenTrade(Uuid::new_v4()),
            dec!(950),
        )
        .expect("reduce trade open");
        assert_eq!(after_open.capital_in_market, dec!(950));
        assert_eq!(after_open.capital_out_market, dec!(50));
        assert_eq!(after_open.total_performance, dec!(-950));

        let after_fee = reduce_trade_projection(
            after_open,
            TransactionCategory::FeeClose(Uuid::new_v4()),
            dec!(5),
        )
        .expect("reduce trade fee");
        assert_eq!(after_fee.total_performance, dec!(-955));

        let after_close = reduce_trade_projection(
            after_fee,
            TransactionCategory::CloseTarget(Uuid::new_v4()),
            dec!(1100),
        )
        .expect("reduce trade close");
        assert_eq!(after_close.capital_in_market, dec!(0));
        assert_eq!(after_close.capital_out_market, dec!(1150));
        assert_eq!(after_close.total_performance, dec!(145));
    }

    #[test]
    fn reduce_trade_projection_applies_tax_payments() {
        let current = sample_trade_balance();

        let after_tax = reduce_trade_projection(
            current,
            TransactionCategory::PaymentTax(Uuid::new_v4()),
            dec!(20),
        )
        .expect("reduce trade tax");

        assert_eq!(after_tax.taxed, dec!(20));
        assert_eq!(after_tax.total_performance, dec!(-20));
    }

    #[test]
    fn reduce_trade_projection_rejects_negative_capital_in_market() {
        let current = sample_trade_balance();

        let error = reduce_trade_projection(
            current,
            TransactionCategory::OpenTrade(Uuid::new_v4()),
            dec!(-1),
        )
        .expect_err("trade capital in market must not project negative");

        assert_eq!(
            error.to_string(),
            "trade projection invariant failed: capital_in_market is negative (-1)"
        );
    }

    #[test]
    fn in_trade_delta_follows_supported_status_transitions() {
        let funded_trade = sample_trade(Status::Funded, dec!(1200), dec!(0));
        assert_eq!(
            in_trade_delta_for_status_transition(&funded_trade, Status::Submitted)
                .expect("funded to submitted delta"),
            Some(dec!(-1200))
        );

        let filled_trade = sample_trade(Status::Filled, dec!(1200), dec!(975));
        assert_eq!(
            in_trade_delta_for_status_transition(&filled_trade, Status::ClosedTarget)
                .expect("filled to closed delta"),
            Some(dec!(-975))
        );

        let submitted_trade = sample_trade(Status::Submitted, dec!(1200), dec!(0));
        assert_eq!(
            in_trade_delta_for_status_transition(&submitted_trade, Status::Filled)
                .expect("submitted to filled delta"),
            None
        );

        let filled_trade = sample_trade(Status::Filled, dec!(1200), dec!(975));
        assert_eq!(
            in_trade_delta_for_status_transition(&filled_trade, Status::Canceled)
                .expect("filled to canceled should not release in-trade capital"),
            None
        );
    }

    #[test]
    fn in_trade_delta_covers_all_terminal_release_paths() {
        let funded_trade = sample_trade(Status::Funded, dec!(1200), Decimal::ZERO);
        assert_eq!(
            in_trade_delta_for_status_transition(&funded_trade, Status::Canceled)
                .expect("funded cancel delta"),
            Some(dec!(-1200))
        );

        for terminal_status in [
            Status::ClosedTarget,
            Status::ClosedStopLoss,
            Status::Expired,
            Status::Rejected,
        ] {
            let filled_trade = sample_trade(Status::Filled, dec!(1200), dec!(975));
            assert_eq!(
                in_trade_delta_for_status_transition(&filled_trade, terminal_status)
                    .expect("filled terminal delta"),
                Some(dec!(-975))
            );

            let canceled_trade = sample_trade(Status::Canceled, dec!(1200), dec!(975));
            assert_eq!(
                in_trade_delta_for_status_transition(&canceled_trade, terminal_status)
                    .expect("canceled terminal delta"),
                Some(dec!(-975))
            );
        }
    }

    #[test]
    fn accumulate_account_projection_matches_stepwise_reduction() {
        let balance = sample_account_balance();
        let trade_id = Uuid::new_v4();
        let updates = [
            (TransactionCategory::FeeOpen(trade_id), dec!(10)),
            (TransactionCategory::OpenTrade(trade_id), dec!(390)),
            (TransactionCategory::PaymentTax(trade_id), dec!(80)),
            (TransactionCategory::Deposit, dec!(120)),
        ];

        let expected = updates
            .iter()
            .try_fold(balance, |state, (category, amount)| {
                reduce_account_projection(state, *category, *amount)
            })
            .expect("sequential account reduction");
        let accumulated =
            accumulate_account_projection(balance, &updates).expect("batched account reduction");

        assert_eq!(accumulated.total_balance, expected.total_balance);
        assert_eq!(accumulated.total_available, expected.total_available);
        assert_eq!(accumulated.total_in_trade, expected.total_in_trade);
        assert_eq!(accumulated.taxed, expected.taxed);
    }

    #[test]
    fn accumulate_trade_projection_matches_stepwise_reduction() {
        let balance = sample_trade_balance();
        let trade_id = Uuid::new_v4();
        let updates = [
            (TransactionCategory::OpenTrade(trade_id), dec!(950)),
            (TransactionCategory::FeeClose(trade_id), dec!(5)),
            (TransactionCategory::CloseTarget(trade_id), dec!(1100)),
            (TransactionCategory::PaymentFromTrade(trade_id), dec!(1150)),
        ];

        let expected = updates
            .iter()
            .try_fold(balance.clone(), |state, (category, amount)| {
                reduce_trade_projection(state, *category, *amount)
            })
            .expect("sequential trade reduction");
        let accumulated =
            accumulate_trade_projection(balance, &updates).expect("batched trade reduction");

        assert_eq!(accumulated.funding, expected.funding);
        assert_eq!(accumulated.capital_in_market, expected.capital_in_market);
        assert_eq!(accumulated.capital_out_market, expected.capital_out_market);
        assert_eq!(accumulated.taxed, expected.taxed);
        assert_eq!(accumulated.total_performance, expected.total_performance);
    }

    #[test]
    fn calculate_account_recomputes_from_persisted_transactions() {
        let mut database = SqliteDatabase::new_in_memory();
        let account = create_sqlite_account(&mut database, "balance-recalc-account");
        let stale_balance = create_sqlite_account_balance(&mut database, &account);
        let trade = create_sqlite_trade(&mut database, &account, "BALANCE_RECALC_ACCOUNT_POSITION");
        let funded_trade = database
            .trade_write()
            .update_trade_status(Status::Funded, &trade)
            .expect("trade should be funded");

        create_sqlite_transaction(
            &mut database,
            &account,
            TransactionCategory::Deposit,
            dec!(1000),
        );
        create_sqlite_transaction(
            &mut database,
            &account,
            TransactionCategory::Withdrawal,
            dec!(150),
        );
        create_sqlite_transaction(
            &mut database,
            &account,
            TransactionCategory::FundTrade(funded_trade.id),
            dec!(300),
        );
        create_sqlite_transaction(
            &mut database,
            &account,
            TransactionCategory::PaymentTax(funded_trade.id),
            dec!(80),
        );
        create_sqlite_transaction(
            &mut database,
            &account,
            TransactionCategory::WithdrawalTax,
            dec!(20),
        );

        let updated = calculate_account(&mut database, &account, &Currency::USD)
            .expect("account balance should be recalculated");

        assert_eq!(updated.id, stale_balance.id);
        assert_eq!(updated.total_balance, dec!(830));
        assert_eq!(updated.total_available, dec!(550));
        assert_eq!(updated.total_in_trade, dec!(300));
        assert_eq!(updated.taxed, dec!(60));

        let persisted = database
            .account_balance_read()
            .for_currency(account.id, &Currency::USD)
            .expect("updated account balance should be persisted");
        assert_eq!(persisted.total_balance, updated.total_balance);
        assert_eq!(persisted.total_available, updated.total_available);
        assert_eq!(persisted.total_in_trade, updated.total_in_trade);
        assert_eq!(persisted.taxed, updated.taxed);
    }

    #[test]
    fn calculate_trade_recomputes_from_persisted_transactions() {
        let mut database = SqliteDatabase::new_in_memory();
        let account = create_sqlite_account(&mut database, "balance-recalc-trade-account");
        create_sqlite_account_balance(&mut database, &account);
        let trade = create_sqlite_trade(&mut database, &account, "BALANCE_RECALC_TRADE_POSITION");

        create_sqlite_transaction(
            &mut database,
            &account,
            TransactionCategory::FundTrade(trade.id),
            dec!(1000),
        );
        create_sqlite_transaction(
            &mut database,
            &account,
            TransactionCategory::OpenTrade(trade.id),
            dec!(950),
        );
        create_sqlite_transaction(
            &mut database,
            &account,
            TransactionCategory::FeeClose(trade.id),
            dec!(5),
        );
        create_sqlite_transaction(
            &mut database,
            &account,
            TransactionCategory::CloseTarget(trade.id),
            dec!(1100),
        );
        create_sqlite_transaction(
            &mut database,
            &account,
            TransactionCategory::PaymentTax(trade.id),
            dec!(20),
        );

        let updated =
            calculate_trade(&mut database, &trade).expect("trade balance should be recalculated");

        assert_eq!(updated.id, trade.balance.id);
        assert_eq!(updated.funding, dec!(1000));
        assert_eq!(updated.capital_in_market, dec!(0));
        assert_eq!(updated.capital_out_market, dec!(1150));
        assert_eq!(updated.taxed, dec!(20));
        assert_eq!(updated.total_performance, dec!(125));

        let persisted = database
            .trade_read()
            .read_trade_balance(trade.balance.id)
            .expect("updated trade balance should be persisted");
        assert_eq!(persisted.funding, updated.funding);
        assert_eq!(persisted.capital_in_market, updated.capital_in_market);
        assert_eq!(persisted.capital_out_market, updated.capital_out_market);
        assert_eq!(persisted.taxed, updated.taxed);
        assert_eq!(persisted.total_performance, updated.total_performance);
    }

    #[test]
    fn empty_projection_batches_return_current_balances_without_writes() {
        let mut database = SqliteDatabase::new_in_memory();
        let account = create_sqlite_account(&mut database, "balance-empty-batch-account");
        let account_balance = create_sqlite_account_balance(&mut database, &account);
        let trade = create_sqlite_trade(&mut database, &account, "BALANCE_EMPTY_BATCH_POSITION");

        let returned_account_balance = apply_account_projection_batch_by_id(
            &mut database,
            account.id,
            &Currency::USD,
            &[],
            Decimal::ZERO,
        )
        .expect("empty account batch should return current balance");
        assert_eq!(returned_account_balance.id, account_balance.id);
        assert_eq!(
            returned_account_balance.updated_at,
            account_balance.updated_at
        );

        let returned_trade_balance = apply_trade_projection_batch(&mut database, &trade, &[])
            .expect("empty trade batch should return current balance");
        assert_eq!(returned_trade_balance.id, trade.balance.id);
        assert_eq!(returned_trade_balance.updated_at, trade.balance.updated_at);
    }

    #[test]
    fn account_in_trade_delta_rejects_negative_projection() {
        let mut database = SqliteDatabase::new_in_memory();
        let account = create_sqlite_account(&mut database, "balance-negative-in-trade-account");
        create_sqlite_account_balance(&mut database, &account);

        let error =
            apply_account_in_trade_delta_by_id(&mut database, account.id, &Currency::USD, dec!(-1))
                .expect_err("negative in-trade projection should fail");

        assert_eq!(
            error.to_string(),
            "account projection invariant failed: total_in_trade is negative (-1)"
        );
    }

    #[test]
    fn account_combined_projection_rejects_negative_in_trade_without_write() {
        let mut database = SqliteDatabase::new_in_memory();
        let account = create_sqlite_account(&mut database, "balance-negative-combined-account");
        let current_balance = create_sqlite_account_balance(&mut database, &account);
        database
            .account_balance_write()
            .update(
                &current_balance,
                dec!(100),
                Decimal::ZERO,
                dec!(100),
                Decimal::ZERO,
            )
            .expect("seed account balance");

        let error = apply_account_projection_with_in_trade_delta_by_id(
            &mut database,
            account.id,
            &Currency::USD,
            TransactionCategory::Deposit,
            dec!(10),
            dec!(-1),
        )
        .expect_err("negative combined in-trade projection should fail");

        assert_eq!(
            error.to_string(),
            "account projection invariant failed: total_in_trade is negative (-1)"
        );
        let persisted = database
            .account_balance_read()
            .for_currency(account.id, &Currency::USD)
            .expect("account balance should remain readable");
        assert_eq!(persisted.total_balance, dec!(100));
        assert_eq!(persisted.total_in_trade, Decimal::ZERO);
        assert_eq!(persisted.total_available, dec!(100));
    }

    #[test]
    fn account_batch_projection_rejects_negative_in_trade_without_write() {
        let mut database = SqliteDatabase::new_in_memory();
        let account = create_sqlite_account(&mut database, "balance-negative-batch-account");
        let current_balance = create_sqlite_account_balance(&mut database, &account);
        database
            .account_balance_write()
            .update(
                &current_balance,
                dec!(100),
                Decimal::ZERO,
                dec!(100),
                Decimal::ZERO,
            )
            .expect("seed account balance");

        let error = apply_account_projection_batch_by_id(
            &mut database,
            account.id,
            &Currency::USD,
            &[(TransactionCategory::Deposit, dec!(10))],
            dec!(-1),
        )
        .expect_err("negative batched in-trade projection should fail");

        assert_eq!(
            error.to_string(),
            "account projection invariant failed: total_in_trade is negative (-1)"
        );
        let persisted = database
            .account_balance_read()
            .for_currency(account.id, &Currency::USD)
            .expect("account balance should remain readable");
        assert_eq!(persisted.total_balance, dec!(100));
        assert_eq!(persisted.total_in_trade, Decimal::ZERO);
        assert_eq!(persisted.total_available, dec!(100));
    }

    #[test]
    fn trade_status_transition_updates_cached_in_trade_projection() {
        let mut database = SqliteDatabase::new_in_memory();
        let account = create_sqlite_account(&mut database, "balance-status-transition-account");
        let balance = create_sqlite_account_balance(&mut database, &account);
        let balance = database
            .account_balance_write()
            .update(&balance, dec!(2000), dec!(1200), dec!(800), Decimal::ZERO)
            .expect("seed funded in-trade projection");

        let mut funded_trade = sample_trade(Status::Funded, dec!(1200), Decimal::ZERO);
        funded_trade.account_id = account.id;

        let updated = apply_account_projection_for_trade_status_transition(
            &mut database,
            &funded_trade,
            Status::Submitted,
        )
        .expect("funded status transition should update")
        .expect("funded transition should release capital");

        assert_eq!(updated.total_in_trade, Decimal::ZERO);

        let balance = database
            .account_balance_write()
            .update(&balance, dec!(2000), dec!(975), dec!(1025), Decimal::ZERO)
            .expect("seed filled in-market projection");
        let mut filled_trade = sample_trade(Status::Filled, dec!(1200), dec!(975));
        filled_trade.account_id = account.id;

        let updated = apply_account_projection_for_trade_status_transition(
            &mut database,
            &filled_trade,
            Status::ClosedStopLoss,
        )
        .expect("filled terminal transition should update")
        .expect("filled transition should release capital");

        assert_eq!(updated.id, balance.id);
        assert_eq!(updated.total_in_trade, Decimal::ZERO);
    }

    #[test]
    fn trade_status_transition_returns_none_for_untracked_transition() {
        let mut database = SqliteDatabase::new_in_memory();
        let account = create_sqlite_account(&mut database, "balance-status-noop-account");
        create_sqlite_account_balance(&mut database, &account);
        let mut submitted_trade = sample_trade(Status::Submitted, dec!(1200), Decimal::ZERO);
        submitted_trade.account_id = account.id;

        let updated = apply_account_projection_for_trade_status_transition(
            &mut database,
            &submitted_trade,
            Status::Filled,
        )
        .expect("untracked transition should be accepted");

        assert!(updated.is_none());
    }
}
