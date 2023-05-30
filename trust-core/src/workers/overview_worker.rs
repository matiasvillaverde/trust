use std::error::Error;
use trust_model::{Account, AccountOverview, Currency, Database, Trade, TradeOverview};

use crate::calculators::TransactionsCalculator;
pub struct OverviewWorker;

impl OverviewWorker {
    pub fn update_account_overview(
        database: &mut dyn Database,
        account: &Account,
        currency: &Currency,
    ) -> Result<AccountOverview, Box<dyn Error>> {
        let total_available =
            TransactionsCalculator::capital_available(account.id, currency, database)?;
        let total_in_trade =
            TransactionsCalculator::capital_in_trades(account.id, currency, database)?; // TODO: there is a bug here
        let taxed = TransactionsCalculator::capital_taxable(account.id, currency, database)?;
        let total_balance = TransactionsCalculator::total_balance(account.id, currency, database)?;

        database.update_account_overview(
            account,
            currency,
            total_balance,
            total_in_trade,
            total_available,
            taxed,
        )
    }

    pub fn update_trade_overview(
        database: &mut dyn Database,
        trade: &Trade,
    ) -> Result<TradeOverview, Box<dyn Error>> {
        let funding = TransactionsCalculator::funding(trade, database)?;
        let capital_in_market = TransactionsCalculator::capital_in_market(trade, database)?;
        let capital_out_market = TransactionsCalculator::capital_out_of_market(trade, database)?;
        let taxed = TransactionsCalculator::taxes(trade, database)?;
        let total_performance = funding - capital_out_market - taxed; // TODO: There is a bug here.

        database.update_trade_overview(
            trade,
            funding,
            capital_in_market,
            capital_out_market,
            taxed,
            total_performance,
        )
    }
}
