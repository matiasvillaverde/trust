use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use std::error::Error;
use trust_model::{Account, AccountOverview, Currency, Database, Trade, TradeOverview};

use crate::calculators::TransactionsCalculator;
pub struct OverviewWorker;

impl OverviewWorker {
    pub fn recalculate_account_overview(
        database: &mut dyn Database,
        account: &Account,
        currency: &Currency,
    ) -> Result<AccountOverview, Box<dyn Error>> {
        let total_available =
            TransactionsCalculator::capital_available(account.id, currency, database)?;
        let total_in_trade =
            TransactionsCalculator::capital_in_trades(account.id, currency, database)?;
        let total_taxable =
            TransactionsCalculator::capital_taxable(account.id, currency, database)?;
        let total_balance = TransactionsCalculator::total_balance(account.id, currency, database)?;

        database.update_account_overview(
            account,
            currency,
            total_balance,
            total_in_trade,
            total_available,
            total_taxable,
        )
    }

    pub fn recalculate_trade_overview(
        database: &mut dyn Database,
        trade: &Trade,
    ) -> Result<TradeOverview, Box<dyn Error>> {
        let total_input = dec!(0);
        let total_in_market = dec!(0);
        let total_out_market = dec!(0);
        let total_taxable = dec!(0);
        let total_performance = dec!(0);

        database.update_trade_overview(
            trade,
            total_input,
            total_in_market,
            total_out_market,
            total_taxable,
            total_performance,
        )
    }
}
