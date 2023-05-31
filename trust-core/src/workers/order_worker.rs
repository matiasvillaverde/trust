use crate::DraftTarget;
use rust_decimal::Decimal;
use trust_model::{
    Currency, DatabaseFactory, Order, OrderAction, ReadTradeDB, Target, Trade, TradeCategory,
    WriteOrderDB,
};
use uuid::Uuid;

pub struct OrderWorker;

impl OrderWorker {
    pub fn create_stop(
        trading_vehicle_id: Uuid,
        quantity: i64,
        price: Decimal,
        currency: &Currency,
        category: &TradeCategory,
        database: &mut dyn DatabaseFactory,
    ) -> Result<Order, Box<dyn std::error::Error>> {
        let tv = database
            .read_trading_vehicle_db()
            .read_trading_vehicle(trading_vehicle_id)?;
        database.write_order_db().create_order(
            &tv,
            quantity,
            price,
            currency,
            &OrderWorker::action_for_stop(category),
        )
    }

    pub fn create_entry(
        trading_vehicle_id: Uuid,
        quantity: i64,
        price: Decimal,
        currency: &Currency,
        category: &TradeCategory,
        database: &mut dyn DatabaseFactory,
    ) -> Result<Order, Box<dyn std::error::Error>> {
        let tv = database
            .read_trading_vehicle_db()
            .read_trading_vehicle(trading_vehicle_id)?;
        database.write_order_db().create_order(
            &tv,
            quantity,
            price,
            currency,
            &OrderWorker::action_for_entry(category),
        )
    }

    pub fn create_target(
        draft: DraftTarget,
        trade: &Trade,
        database: &mut dyn DatabaseFactory,
    ) -> Result<Target, Box<dyn std::error::Error>> {
        let tv = database
            .read_trading_vehicle_db()
            .read_trading_vehicle(trade.trading_vehicle.id)?;
        let order = database.write_order_db().create_order(
            &tv,
            draft.quantity,
            draft.order_price,
            &trade.currency,
            &OrderWorker::action_for_target(&trade.category),
        )?;

        database
            .write_order_db()
            .create_target(draft.target_price, &trade.currency, &order, trade)
    }

    pub fn record_timestamp_entry(
        trade: &Trade,
        write_database: &mut dyn WriteOrderDB,
        read_database: &mut dyn ReadTradeDB,
    ) -> Result<Trade, Box<dyn std::error::Error>> {
        write_database.record_order_opening(&trade.entry)?;
        read_database.read_trade(trade.id)
    }

    pub fn record_timestamp_stop(
        trade: &Trade,
        write_database: &mut dyn WriteOrderDB,
        read_database: &mut dyn ReadTradeDB,
    ) -> Result<Trade, Box<dyn std::error::Error>> {
        write_database.record_order_closing(&trade.safety_stop)?;
        read_database.read_trade(trade.id)
    }

    pub fn record_timestamp_target(
        trade: &Trade,
        write_database: &mut dyn WriteOrderDB,
        read_database: &mut dyn ReadTradeDB,
    ) -> Result<Trade, Box<dyn std::error::Error>> {
        write_database.record_order_closing(&trade.exit_targets.first().unwrap().order)?;
        read_database.read_trade(trade.id)
    }

    fn action_for_stop(category: &TradeCategory) -> OrderAction {
        match category {
            TradeCategory::Long => OrderAction::Sell,
            TradeCategory::Short => OrderAction::Buy,
        }
    }

    fn action_for_entry(category: &TradeCategory) -> OrderAction {
        match category {
            TradeCategory::Long => OrderAction::Buy,
            TradeCategory::Short => OrderAction::Sell,
        }
    }

    fn action_for_target(category: &TradeCategory) -> OrderAction {
        match category {
            TradeCategory::Long => OrderAction::Sell,
            TradeCategory::Short => OrderAction::Buy,
        }
    }
}
