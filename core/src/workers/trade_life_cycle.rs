use crate::TransactionWorker;
use model::{
    AccountOverview, Broker, BrokerLog, DatabaseFactory, Status, Trade, TradeOverview, Transaction,
};

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

    pub fn submit_trade(
        trade: &Trade,
        database: &mut dyn DatabaseFactory,
        broker: &mut dyn Broker,
    ) -> Result<(Trade, BrokerLog), Box<dyn std::error::Error>> {
        // 1. Validate that Trade can be submitted
        crate::validators::trade::can_submit(trade)?;

        // 2. Submit trade to broker
        let account = database.account_read().id(trade.account_id)?;
        let (log, order_id) = broker.submit_trade(trade, &account)?;

        // 3. Save log in the DB
        database.log_write().create_log(log.log.as_str(), trade)?;

        // 4. Update Trade status to submitted
        let trade = database
            .trade_write()
            .update_trade_status(Status::Submitted, trade)?;

        // 5. Update internal orders orders to submitted
        database
            .order_write()
            .submit_of(&trade.safety_stop, order_id.stop)?;
        database
            .order_write()
            .submit_of(&trade.entry, order_id.entry)?;
        database
            .order_write()
            .submit_of(&trade.target, order_id.target)?;

        // 6. Read Trade with updated values
        let trade = database.trade_read().read_trade(trade.id)?;

        // 7. Return Trade and Log
        Ok((trade, log))
    }
}
