use crate::{OrderStatus, OrderWorker, OverviewWorker, TradeAction, TransactionWorker};
use model::{
    Account, AccountOverview, Broker, BrokerLog, DatabaseFactory, Order, Status, Trade,
    TradeOverview, Transaction,
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

    pub fn sync_trade(
        trade: &Trade,
        account: &Account,
        database: &mut dyn DatabaseFactory,
        broker: &mut dyn Broker,
    ) -> Result<(Status, Vec<Order>, BrokerLog), Box<dyn std::error::Error>> {
        // 1. Sync Trade with Broker
        let (status, orders, log) = broker.sync_trade(trade, account)?;

        // 2. Save log in the DB
        database.log_write().create_log(log.log.as_str(), trade)?;

        // 3. Update Orders
        for order in orders.clone() {
            OrderWorker::update_order(&order, database)?;
        }

        // 4. Update Trade Status
        let trade = database.trade_read().read_trade(trade.id)?; // We need to read the trade again to get the updated orders
        TradeAction::update_status(&trade, status, database)?;

        // 5. Update Account Overview
        OverviewWorker::calculate_account(database, account, &trade.currency)?;

        Ok((status, orders, log))
    }

    pub fn close_trade(
        trade: &Trade,
        database: &mut dyn DatabaseFactory,
        broker: &mut dyn Broker,
    ) -> Result<(TradeOverview, BrokerLog), Box<dyn std::error::Error>> {
        // 1. Verify trade can be closed
        crate::validators::trade::can_close(trade)?;

        // 2. Submit a market order to close the trade
        let account = database.account_read().id(trade.account_id)?;
        let (target_order, log) = broker.close_trade(trade, &account)?;

        // 3. Save log in the database
        database.log_write().create_log(log.log.as_str(), trade)?;

        // 4. Update Order Target with the filled price and new ID
        OrderWorker::update_order(&target_order, database)?;

        // 5. Update Trade Status
        database
            .trade_write()
            .update_trade_status(Status::Canceled, trade)?;

        // 6. Cancel Stop-loss Order
        let mut stop_order = trade.safety_stop.clone();
        stop_order.status = OrderStatus::Canceled;
        database.order_write().update(&stop_order)?;

        Ok((trade.overview.clone(), log))
    }
}
