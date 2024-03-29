use model::{Account, AccountBalance, Currency, DatabaseFactory, Trade, TradeBalance};
use std::error::Error;

use crate::{
    calculators_account::{
        AccountCapitalAvailable, AccountCapitalBalance, AccountCapitalInApprovedTrades,
        AccountCapitalTaxable,
    },
    calculators_trade::{TradeCapitalFunded, TradeCapitalInMarket},
    calculators_trade::{TradeCapitalOutOfMarket, TradeCapitalTaxable, TradePerformance},
};

pub fn calculate_account(
    database: &mut dyn DatabaseFactory,
    account: &Account,
    currency: &Currency,
) -> Result<AccountBalance, Box<dyn Error>> {
    let total_available = AccountCapitalAvailable::calculate(
        account.id,
        currency,
        database.transaction_read().as_mut(),
    )?;
    let total_in_trade = AccountCapitalInApprovedTrades::calculate(
        account.id,
        currency,
        database.transaction_read().as_mut(),
    )?;
    let taxed = AccountCapitalTaxable::calculate(
        account.id,
        currency,
        database.transaction_read().as_mut(),
    )?;
    let total_balance = AccountCapitalBalance::calculate(
        account.id,
        currency,
        database.transaction_read().as_mut(),
    )?;

    let balance = database
        .account_balance_read()
        .for_currency(account.id, currency)?;

    database.account_balance_write().update(
        &balance,
        total_balance,
        total_in_trade,
        total_available,
        taxed,
    )
}

pub fn calculate_trade(
    database: &mut dyn DatabaseFactory,
    trade: &Trade,
) -> Result<TradeBalance, Box<dyn Error>> {
    let funding = TradeCapitalFunded::calculate(trade.id, database.transaction_read().as_mut())?;
    let capital_in_market =
        TradeCapitalInMarket::calculate(trade.id, database.transaction_read().as_mut())?;
    let capital_out_market =
        TradeCapitalOutOfMarket::calculate(trade.id, database.transaction_read().as_mut())?;
    let taxed = TradeCapitalTaxable::calculate(trade.id, database.transaction_read().as_mut())?;
    let total_performance =
        TradePerformance::calculate(trade.id, database.transaction_read().as_mut())?;

    database.trade_balance_write().update_trade_balance(
        trade,
        funding,
        capital_in_market,
        capital_out_market,
        taxed,
        total_performance,
    )
}
