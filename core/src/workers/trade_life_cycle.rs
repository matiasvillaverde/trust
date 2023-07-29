use crate::TransactionWorker;
use model::{AccountOverview, DatabaseFactory, Status, Trade, TradeOverview, Transaction};

pub struct TradeLifecycle;

impl TradeLifecycle {
    pub fn fund_trade(
        trade: &Trade,
        database: &mut dyn DatabaseFactory,
    ) -> Result<(Trade, Transaction, AccountOverview, TradeOverview), Box<dyn std::error::Error>>
    {
        // 1. Validate that trade can be funded
        crate::validators::funding::can_fund(trade, database)?;

        // 2. Update trade status to funded
        database
            .trade_write()
            .update_trade_status(Status::Funded, trade)?;

        // 3. Create transaction to fund the trade
        let (transaction, account_overview, trade_overview) =
            TransactionWorker::transfer_to_fund_trade(trade, database)?;

        // 4. Return data objects
        Ok((trade.clone(), transaction, account_overview, trade_overview))
    }
}
