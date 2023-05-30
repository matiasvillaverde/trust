use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use std::error::Error;
use trust_model::{Account, AccountOverview, Currency, Database};

use crate::calculators::TransactionsCalculator;
pub struct OverviewWorker;

impl OverviewWorker {
    pub fn recalculate_account_overview(
        database: &mut dyn Database,
        account: &Account,
        currency: &Currency,
    ) -> Result<AccountOverview, Box<dyn Error>> {
        let total_available = TransactionsCalculator::calculate_total_capital_available(
            account.id, currency, database,
        )?;
        let total_in_trade = TransactionsCalculator::calculate_total_capital_in_trade(
            account.id, currency, database,
        )?;
        let total_taxable =
            TransactionsCalculator::calculate_total_taxable(account.id, currency, database)?;
        let total_balance = dec!(0);

        database.update_account_overview(
            account,
            currency,
            total_balance,
            total_in_trade,
            total_available,
            total_taxable,
        )
    }
}
