use crate::order::Order;
use crate::price::Price;
use chrono::NaiveDateTime;
use uuid::Uuid;

/// Target entity - represents a target price for a trade. Trades can have multiple targets.
#[derive(PartialEq, Debug)]
pub struct Target {
    pub id: Uuid,

    // Entity timestamps
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
    pub deleted_at: Option<NaiveDateTime>,

    // Entity fields
    /// The price that needs to be reached before the order is placed. The price can be different from the order price.
    /// For example, if you want to buy/sell an asset at a specific price, you can set the target_price to be the same as the order price.
    /// If you want to buy/sell an asset at a specific price or better, you can set the target_price to be different from the order price.
    /// This is used to secure part of the profit.
    pub target_price: Price,

    /// The order that should be submitted once the target price is reached.
    pub order: Order,

    /// The trade that the target is associated with.
    pub trade_uuid: Uuid,
}
