use rust_decimal::Decimal;
use trust_model::{Currency, Database, Order, OrderAction, TradeCategory};
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
        database.create_stop(
            &tv,
            quantity,
            price,
            currency,
            &OrderWorker::action_for(&category),
        )
    }

    fn action_for(category: &TradeCategory) -> OrderAction {
        match category {
            TradeCategory::Long => OrderAction::Sell,
            TradeCategory::Short => OrderAction::Buy,
        }
    }
}
