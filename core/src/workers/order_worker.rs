use model::{
    Currency, DatabaseFactory, Order, OrderAction, OrderCategory, ReadTradeDB, Trade,
    TradeCategory, WriteOrderDB,
};
use rust_decimal::Decimal;
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
            &action_for_stop(category),
            &OrderCategory::Market,
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
            &action_for_entry(category),
            &OrderCategory::Limit,
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

        let action = action_for_target(category);

        database.write_order_db().create_order(
            &tv,
            quantity,
            price,
            currency,
            &action,
            &OrderCategory::Limit,
        )
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_action_for_stop_long() {
        let category = TradeCategory::Long;
        assert_eq!(action_for_stop(&category), OrderAction::Sell);
    }

    #[test]
    fn test_action_for_stop_short() {
        let category = TradeCategory::Short;
        assert_eq!(action_for_stop(&category), OrderAction::Buy);
    }

    #[test]
    fn test_action_for_entry_long() {
        let category = TradeCategory::Long;
        assert_eq!(action_for_entry(&category), OrderAction::Buy);
    }

    #[test]
    fn test_action_for_entry_short() {
        let category = TradeCategory::Short;
        assert_eq!(action_for_entry(&category), OrderAction::Sell);
    }

    #[test]
    fn test_action_for_target_long() {
        let category = TradeCategory::Long;
        assert_eq!(action_for_target(&category), OrderAction::Sell);
    }

    #[test]
    fn test_action_for_target_short() {
        let category = TradeCategory::Short;
        assert_eq!(action_for_target(&category), OrderAction::Buy);
    }
}
