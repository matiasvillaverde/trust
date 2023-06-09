use rust_decimal::Decimal;
use trust_model::{
    Currency, DatabaseFactory, Order, OrderAction, ReadTradeDB, Trade, TradeCategory, WriteOrderDB,
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

    pub fn update_order(
        order: &Order,
        database: &mut dyn DatabaseFactory,
    ) -> Result<Order, Box<dyn std::error::Error>> {
        database.write_order_db().update_order(order)
    }

    pub fn create_target(
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

        let action = OrderWorker::action_for_target(category);

        database
            .write_order_db()
            .create_order(&tv, quantity, price, currency, &action)
    }

    pub fn record_timestamp_filled(
        trade: &Trade,
        write_database: &mut dyn WriteOrderDB,
        read_database: &mut dyn ReadTradeDB,
    ) -> Result<Trade, Box<dyn std::error::Error>> {
        write_database.record_filled(&trade.entry)?;
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
        write_database.record_order_closing(&trade.target)?;
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
