use model::{Account, AccountOverview, Currency, DatabaseFactory, Trade, TradeOverview};
use std::error::Error;

use crate::{
    account_calculators::{
        AccountCapitalAvailable, AccountCapitalBalance, AccountCapitalInApprovedTrades,
        AccountCapitalTaxable,
    },
    trade_calculators::{TradeCapitalFunded, TradeCapitalInMarket},
    trade_calculators::{TradeCapitalOutOfMarket, TradeCapitalTaxable, TradePerformance},
};
pub struct OverviewWorker;

impl OverviewWorker {
    pub fn calculate_account(
        database: &mut dyn DatabaseFactory,
        account: &Account,
        currency: &Currency,
    ) -> Result<AccountOverview, Box<dyn Error>> {
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

        let overview = database
            .account_overview_read()
            .for_currency(account.id, currency)?;

        database.account_overview_write().update(
            &overview,
            total_balance,
            total_in_trade,
            total_available,
            taxed,
        )
    }

    pub fn calculate_trade(
        database: &mut dyn DatabaseFactory,
        trade: &Trade,
    ) -> Result<TradeOverview, Box<dyn Error>> {
        let funding =
            TradeCapitalFunded::calculate(trade.id, database.transaction_read().as_mut())?;
        let capital_in_market =
            TradeCapitalInMarket::calculate(trade.id, database.transaction_read().as_mut())?;
        let capital_out_market =
            TradeCapitalOutOfMarket::calculate(trade.id, database.transaction_read().as_mut())?;
        let taxed = TradeCapitalTaxable::calculate(trade.id, database.transaction_read().as_mut())?;
        let total_performance =
            TradePerformance::calculate(trade.id, database.transaction_read().as_mut())?;

        database.trade_overview_write().update_trade_overview(
            trade,
            funding,
            capital_in_market,
            capital_out_market,
            taxed,
            total_performance,
        )
    }
}
