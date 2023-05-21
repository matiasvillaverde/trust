use rust_decimal::Decimal;
use trust_model::{Currency, Database, Order, OrderAction, Target, Trade, TradeCategory};
use uuid::Uuid;

pub struct OrderWorker;

impl OrderWorker {
    pub fn create_stop(
        trading_vehicle_id: Uuid,
        quantity: i64,
        price: Decimal,
        currency: &Currency,
        category: &TradeCategory,
        database: &mut dyn Database,
    ) -> Result<Order, Box<dyn std::error::Error>> {
        let tv = database.read_trading_vehicle(trading_vehicle_id)?;
        database.create_order(
            &tv,
            quantity,
            price,
            currency,
            &OrderWorker::action_for_stop(&category),
        )
    }

    pub fn create_entry(
        trading_vehicle_id: Uuid,
        quantity: i64,
        price: Decimal,
        currency: &Currency,
        category: &TradeCategory,
        database: &mut dyn Database,
    ) -> Result<Order, Box<dyn std::error::Error>> {
        let tv = database.read_trading_vehicle(trading_vehicle_id)?;
        database.create_order(
            &tv,
            quantity,
            price,
            currency,
            &OrderWorker::action_for_entry(&category),
        )
    }

    pub fn create_target(
        trading_vehicle_id: Uuid,
        quantity: i64,
        order_price: Decimal,
        currency: &Currency,
        target_price: Decimal,
        category: &TradeCategory,
        trade: &Trade,
        database: &mut dyn Database,
    ) -> Result<Target, Box<dyn std::error::Error>> {
        let tv = database.read_trading_vehicle(trading_vehicle_id)?;
        let order = database.create_order(
            &tv,
            quantity,
            order_price,
            currency,
            &OrderWorker::action_for_target(&category),
        )?;

        database.create_target(target_price, currency, &order, trade)
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
