use crate::price::Price;
use chrono::NaiveDateTime;
use uuid::Uuid;

#[derive(PartialEq, Debug)]
pub struct Order {
    pub id: Uuid,

    // Entity timestamps
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
    pub deleted_at: Option<NaiveDateTime>,

    // Entity fields
    /// The unit price of the order
    pub unit_price: Price,

    /// The quantity of the order
    pub quantity: u64,

    /// The trading vehicle ID - the asset that is traded
    pub trading_vehicle_id: Uuid,

    /// The category of the order - market, limit, stop, etc. It depends on the exchange.
    pub category: OrderCategory,

    /// The action of the order - buy, sell, short, etc.
    pub action: OrderAction,

    // Lifecycle fields
    /// When the order was opened in an exchange
    pub open_at: Option<NaiveDateTime>,

    /// When the order was closed in an exchange
    pub closed_at: Option<NaiveDateTime>,
}

/// The category of the order - market, limit, stop, etc. It depends on the exchange.
#[derive(PartialEq, Debug)]
pub enum OrderCategory {
    /// Market order - buy or sell at the current market price. The order is executed immediately.
    Market,

    /// Limit order - buy or sell at a specific price or better. The order is executed when the price is reached.
    Limit,

    /// Stop order - buy or sell at a specific price or worse. The order is executed when the price is reached.
    Stop,
}

/// The action of the order - buy, sell, short, etc.
#[derive(PartialEq, Debug)]
pub enum OrderAction {
    /// Sell an asset that you own
    Sell,

    /// Buy an asset with money that you have
    Buy,

    /// Sell an asset that you don't own
    Short,
}
