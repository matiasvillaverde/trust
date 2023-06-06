use crate::price::Price;
use chrono::NaiveDateTime;
use chrono::Utc;
use rust_decimal::Decimal;
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

    /// The time in force of the order - day, until canceled, etc.
    pub time_in_force: TimeInForce,

    /// For Trailing Orders - the trailing percent
    pub trailing_percent: Option<Decimal>,

    /// For Trailing Orders - the trailing price
    pub trailing_price: Option<Decimal>,

    /// The quantity of the order
    pub filled_quantity: u64,

    /// The average filled price of the order
    pub average_filled_price: Option<Decimal>,

    /// If true, the order is eligible for execution outside regular
    /// trading hours.
    pub extended_hours: bool,

    // Lifecycle fields
    /// When the order was submitted to the broker
    pub submitted_at: Option<NaiveDateTime>,

    /// When the order was filled in an broker
    pub filled_at: Option<NaiveDateTime>,

    /// When the order was expired in an broker
    pub expired_at: Option<NaiveDateTime>,

    /// When the order was canceled in an broker
    pub cancelled_at: Option<NaiveDateTime>,

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

#[derive(PartialEq, Debug, Clone, Copy, Default)]
pub enum TimeInForce {
    /// The order is good for the day, and it will be canceled
    /// automatically at the end of Regular Trading Hours if unfilled.
    Day,
    /// The order is good until canceled.
    #[default]
    UntilCanceled,
    /// This order is eligible to execute only in the market opening
    /// auction. Any unfilled orders after the open will be canceled.
    UntilMarketOpen,
    /// This order is eligible to execute only in the market closing
    /// auction. Any unfilled orders after the close will be canceled.
    UntilMarketClose,
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

impl std::fmt::Display for TimeInForce {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            TimeInForce::Day => write!(f, "day"),
            TimeInForce::UntilCanceled => write!(f, "until_canceled"),
            TimeInForce::UntilMarketOpen => write!(f, "until_market_open"),
            TimeInForce::UntilMarketClose => write!(f, "until_market_close"),
        }
    }
}
#[derive(PartialEq, Debug)]
pub struct TimeInForceParseError;
impl std::str::FromStr for TimeInForce {
    type Err = TimeInForceParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "day" => Ok(TimeInForce::Day),
            "until_canceled" => Ok(TimeInForce::UntilCanceled),
            "until_market_open" => Ok(TimeInForce::UntilMarketOpen),
            "until_market_close" => Ok(TimeInForce::UntilMarketClose),
            _ => Err(TimeInForceParseError),
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

impl Default for Order {
    fn default() -> Self {
        let now = Utc::now().naive_utc();
        Order {
            id: Uuid::new_v4(),
            created_at: now,
            updated_at: now,
            deleted_at: None,
            unit_price: Price::default(),
            trading_vehicle_id: Uuid::new_v4(),
            action: OrderAction::Buy,
            category: OrderCategory::Market,
            time_in_force: TimeInForce::default(),
            quantity: 10,
            filled_quantity: 0,
            average_filled_price: None,
            extended_hours: false,
            submitted_at: None,
            filled_at: None,
            expired_at: None,
            cancelled_at: None,
            closed_at: None,
            trailing_percent: None,
            trailing_price: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_order_category_parse() {
        assert_eq!("market".parse::<OrderCategory>(), Ok(OrderCategory::Market));
        assert_eq!("limit".parse::<OrderCategory>(), Ok(OrderCategory::Limit));
        assert_eq!("stop".parse::<OrderCategory>(), Ok(OrderCategory::Stop));
        assert!("invalid".parse::<OrderCategory>().is_err());
    }

    #[test]
    fn test_order_category_display() {
        assert_eq!(format!("{}", OrderCategory::Market), "market");
        assert_eq!(format!("{}", OrderCategory::Limit), "limit");
        assert_eq!(format!("{}", OrderCategory::Stop), "stop");
    }
}
