use std::error::Error;
use trust_model::{Account, AccountOverview, Currency, DatabaseFactory, Trade, TradeOverview};

use crate::calculators::{CapitalAvailableCalculator, TransactionsCalculator};
pub struct OverviewWorker;

impl OverviewWorker {
    pub fn update_account_overview(
        database: &mut dyn DatabaseFactory,
        account: &Account,
        currency: &Currency,
    ) -> Result<AccountOverview, Box<dyn Error>> {
        let total_available = CapitalAvailableCalculator::capital_available(
            account.id,
            currency,
            database.read_transaction_db().as_mut(),
        )?;
        let total_in_trade = TransactionsCalculator::capital_in_trades(
            account.id,
            currency,
            database.read_transaction_db().as_mut(),
        )?;
        let taxed = TransactionsCalculator::capital_taxable(
            account.id,
            currency,
            database.read_transaction_db().as_mut(),
        )?;
        let total_balance = TransactionsCalculator::total_balance(
            account.id,
            currency,
            database.read_transaction_db().as_mut(),
        )?;

        database
            .write_account_overview_db()
            .update_account_overview(
                account,
                currency,
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
            TransactionsCalculator::funding(trade, database.read_transaction_db().as_mut())?;
        let capital_in_market = TransactionsCalculator::capital_in_market(
            trade,
            database.read_transaction_db().as_mut(),
        )?;
        let capital_out_market = TransactionsCalculator::capital_out_of_market(
            trade,
            database.read_transaction_db().as_mut(),
        )?;
        let taxed = TransactionsCalculator::taxes(trade, database.read_transaction_db().as_mut())?;
        let total_performance = TransactionsCalculator::total_performance(
            trade,
            database.read_transaction_db().as_mut(),
        )?;

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
