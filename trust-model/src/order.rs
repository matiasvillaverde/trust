use crate::price::Price;
use chrono::NaiveDateTime;
use uuid::Uuid;

/// Order entity - represents a single order. Orders can be part of a trade.
///
/// Orders can be entries to the market or exits from the market.
/// Orders are part of a trade entries and exits.
#[derive(PartialEq, Debug, Clone)]
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
    /// When the order was filled in an exchange
    pub opened_at: Option<NaiveDateTime>,

    /// When the order was closed in an exchange
    pub closed_at: Option<NaiveDateTime>,
}

/// The category of the order - market, limit, stop, etc. It depends on the exchange.
#[derive(PartialEq, Debug, Clone, Copy)]
pub enum OrderCategory {
    /// Market order - buy or sell at the current market price. The order is executed immediately.
    Market,

    /// Limit order - buy or sell at a specific price or better. The order is executed when the price is reached.
    Limit,

    /// Stop order - buy or sell at a specific price or worse. The order is executed when the price is reached.
    Stop,
}

/// The action of the order - buy, sell, short, etc.
#[derive(PartialEq, Debug, Clone, Copy)]
pub enum OrderAction {
    /// Sell an asset that you own
    Sell,

    /// Buy an asset with money that you have
    Buy,

    /// Sell an asset that you don't own
    Short,
}

impl std::fmt::Display for OrderCategory {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            OrderCategory::Market => write!(f, "market"),
            OrderCategory::Limit => write!(f, "limit"),
            OrderCategory::Stop => write!(f, "stop"),
        }
    }
}

#[derive(PartialEq, Debug)]
pub struct OrderCategoryParseError;
impl std::str::FromStr for OrderCategory {
    type Err = OrderCategoryParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "market" => Ok(OrderCategory::Market),
            "limit" => Ok(OrderCategory::Limit),
            "stop" => Ok(OrderCategory::Stop),
            _ => Err(OrderCategoryParseError),
        }
    }
}

// Implementations
impl std::fmt::Display for OrderAction {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            OrderAction::Sell => write!(f, "sell"),
            OrderAction::Buy => write!(f, "buy"),
            OrderAction::Short => write!(f, "short"),
        }
    }
}

#[derive(PartialEq, Debug)]
pub struct OrderActionParseError;
impl std::str::FromStr for OrderAction {
    type Err = OrderActionParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "sell" => Ok(OrderAction::Sell),
            "buy" => Ok(OrderAction::Buy),
            "short" => Ok(OrderAction::Short),
            _ => Err(OrderActionParseError),
        }
    }
}
