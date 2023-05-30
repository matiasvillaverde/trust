use crate::DraftTarget;
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
            &OrderWorker::action_for_stop(category),
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
            &OrderWorker::action_for_entry(category),
        )
    }

    pub fn create_target(
        draft: DraftTarget,
        trade: &Trade,
        database: &mut dyn Database,
    ) -> Result<Target, Box<dyn std::error::Error>> {
        let tv = database.read_trading_vehicle(trade.trading_vehicle.id)?;
        let order = database.create_order(
            &tv,
            draft.quantity,
            draft.order_price,
            &trade.currency,
            &OrderWorker::action_for_target(&trade.category),
        )?;

        database.create_target(draft.target_price, &trade.currency, &order, trade)
    }

    pub fn record_timestamp_entry(
        trade: &Trade,
        database: &mut dyn Database,
    ) -> Result<Trade, Box<dyn std::error::Error>> {
        database.record_order_opening(&trade.entry)?;
        database.read_trade(trade.id)
    }

    pub fn record_timestamp_stop(
        trade: &Trade,
        database: &mut dyn Database,
    ) -> Result<Trade, Box<dyn std::error::Error>> {
        database.record_order_closing(&trade.safety_stop)?;
        database.read_trade(trade.id)
    }

    pub fn record_timestamp_target(
        trade: &Trade,
        database: &mut dyn Database,
    ) -> Result<Trade, Box<dyn std::error::Error>> {
        database.record_order_closing(&trade.exit_targets.first().unwrap().order)?;
        database.read_trade(trade.id)
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
