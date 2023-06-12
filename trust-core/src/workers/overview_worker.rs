use std::error::Error;
use trust_model::{Account, AccountOverview, Currency, DatabaseFactory, Trade, TradeOverview};

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
    pub fn update_account_overview(
        database: &mut dyn DatabaseFactory,
        account: &Account,
        currency: &Currency,
    ) -> Result<AccountOverview, Box<dyn Error>> {
        let total_available = AccountCapitalAvailable::calculate(
            account.id,
            currency,
            database.read_transaction_db().as_mut(),
        )?;
        let total_in_trade = AccountCapitalInApprovedTrades::calculate(
            account.id,
            currency,
            database.read_transaction_db().as_mut(),
        )?;
        let taxed = AccountCapitalTaxable::calculate(
            account.id,
            currency,
            database.read_transaction_db().as_mut(),
        )?;
        let total_balance = AccountCapitalBalance::calculate(
            account.id,
            currency,
            database.read_transaction_db().as_mut(),
        )?;

        let overview = database
            .read_account_overview_db()
            .read_account_overview_currency(account.id, currency)?;

        database
            .write_account_overview_db()
            .update_account_overview(
                &overview,
                total_balance,
                total_in_trade,
                total_available,
                taxed,
            )
    }

    pub fn update_trade_overview(
        database: &mut dyn DatabaseFactory,
        trade: &Trade,
    ) -> Result<TradeOverview, Box<dyn Error>> {
        let funding =
            TradeCapitalFunded::calculate(trade.id, database.read_transaction_db().as_mut())?;
        let capital_in_market =
            TradeCapitalInMarket::calculate(trade.id, database.read_transaction_db().as_mut())?;
        let capital_out_market =
            TradeCapitalOutOfMarket::calculate(trade.id, database.read_transaction_db().as_mut())?;
        let taxed =
            TradeCapitalTaxable::calculate(trade.id, database.read_transaction_db().as_mut())?;
        let total_performance =
            TradePerformance::calculate(trade.id, database.read_transaction_db().as_mut())?;

        database.write_trade_overview_db().update_trade_overview(
            trade,
            funding,
            capital_in_market,
            capital_out_market,
            taxed,
            total_performance,
        )
    }
}
