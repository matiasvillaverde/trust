use chrono::NaiveDateTime;
use chrono::Utc;
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use std::str::FromStr;
use uuid::Uuid;
/// Order entity - represents a single order. Orders can be part of a trade.
///
/// Orders can be entries to the market or exits from the market.
/// Orders are part of a trade entries and exits.
#[derive(PartialEq, Debug, Clone)]
pub struct Order {
    pub id: Uuid,

    /// The id of the order in the broker
    pub broker_order_id: Option<Uuid>,

    // Entity timestamps
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
    pub deleted_at: Option<NaiveDateTime>,

    /// The unit price of the order
    pub unit_price: Decimal,

    /// The quantity of the order
    pub quantity: u64,

    /// The trading vehicle ID - the asset that is traded
    pub trading_vehicle_id: Uuid,

    /// The category of the order - market, limit, stop, etc. It depends on the exchange.
    pub category: OrderCategory,

    /// The action of the order - buy, sell, short, etc.
    pub action: OrderAction,

    /// The status of the order - open, filled, canceled, etc.
    pub status: OrderStatus,

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

/// The status an order can have.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OrderStatus {
    /// The order has been received by Broker, and routed to exchanges for
    /// execution. This is the usual initial state of an order.
    New,
    /// The order has changed.
    Replaced,
    /// The order has been partially filled.
    PartiallyFilled,
    /// The order has been filled, and no further updates will occur for
    /// the order.
    Filled,
    /// The order is done executing for the day, and will not receive
    /// further updates until the next trading day.
    DoneForDay,
    /// The order has been canceled, and no further updates will occur for
    /// the order. This can be either due to a cancel request by the user,
    /// or the order has been canceled by the exchanges due to its
    /// time-in-force.
    Canceled,
    /// The order has expired, and no further updates will occur for the
    /// order.
    Expired,
    /// The order has been received by Broker, but hasn't yet been routed
    /// to the execution venue. This state only occurs on rare occasions.
    Accepted,
    /// The order has been received by Broker, and routed to the
    /// exchanges, but has not yet been accepted for execution. This state
    /// only occurs on rare occasions.
    PendingNew,
    /// The order has been received by exchanges, and is evaluated for
    /// pricing. This state only occurs on rare occasions.
    AcceptedForBidding,
    /// The order is waiting to be canceled. This state only occurs on
    /// rare occasions.
    PendingCancel,
    /// The order is awaiting replacement.
    PendingReplace,
    /// The order has been stopped, and a trade is guaranteed for the
    /// order, usually at a stated price or better, but has not yet
    /// occurred. This state only occurs on rare occasions.
    Stopped,
    /// The order has been rejected, and no further updates will occur for
    /// the order. This state occurs on rare occasions and may occur based
    /// on various conditions decided by the exchanges.
    Rejected,
    /// The order has been suspended, and is not eligible for trading.
    /// This state only occurs on rare occasions.
    Suspended,
    /// The order has been completed for the day (either filled or done
    /// for day), but remaining settlement calculations are still pending.
    /// This state only occurs on rare occasions.
    Calculated,
    /// The order is still being held. This may be the case for legs of
    /// bracket-style orders that are not active yet because the primary
    /// order has not filled yet.
    Held,
    /// Any other status that we have not accounted for.
    ///
    /// Note that having any such status should be considered a bug.
    Unknown,
}

impl std::fmt::Display for OrderStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            OrderStatus::New => write!(f, "new"),
            OrderStatus::Replaced => write!(f, "replaced"),
            OrderStatus::PartiallyFilled => write!(f, "partially_filled"),
            OrderStatus::Filled => write!(f, "filled"),
            OrderStatus::DoneForDay => write!(f, "done_for_day"),
            OrderStatus::Canceled => write!(f, "canceled"),
            OrderStatus::Expired => write!(f, "expired"),
            OrderStatus::Accepted => write!(f, "accepted"),
            OrderStatus::PendingNew => write!(f, "pending_new"),
            OrderStatus::AcceptedForBidding => write!(f, "accepted_for_bidding"),
            OrderStatus::PendingCancel => write!(f, "pending_cancel"),
            OrderStatus::PendingReplace => write!(f, "pending_replace"),
            OrderStatus::Stopped => write!(f, "stopped"),
            OrderStatus::Rejected => write!(f, "rejected"),
            OrderStatus::Suspended => write!(f, "suspended"),
            OrderStatus::Calculated => write!(f, "calculated"),
            OrderStatus::Held => write!(f, "held"),
            OrderStatus::Unknown => write!(f, "unknown"),
        }
    }
}

#[derive(PartialEq, Debug)]
pub struct OrderStatusParseError;
impl FromStr for OrderStatus {
    type Err = OrderStatusParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "new" => Ok(OrderStatus::New),
            "replaced" => Ok(OrderStatus::Replaced),
            "partially_filled" => Ok(OrderStatus::PartiallyFilled),
            "filled" => Ok(OrderStatus::Filled),
            "done_for_day" => Ok(OrderStatus::DoneForDay),
            "canceled" => Ok(OrderStatus::Canceled),
            "expired" => Ok(OrderStatus::Expired),
            "accepted" => Ok(OrderStatus::Accepted),
            "pending_new" => Ok(OrderStatus::PendingNew),
            "accepted_for_bidding" => Ok(OrderStatus::AcceptedForBidding),
            "pending_cancel" => Ok(OrderStatus::PendingCancel),
            "pending_replace" => Ok(OrderStatus::PendingReplace),
            "stopped" => Ok(OrderStatus::Stopped),
            "rejected" => Ok(OrderStatus::Rejected),
            "suspended" => Ok(OrderStatus::Suspended),
            "calculated" => Ok(OrderStatus::Calculated),
            "held" => Ok(OrderStatus::Held),
            "unknown" => Ok(OrderStatus::Unknown),
            _ => Err(OrderStatusParseError),
        }
    }
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
            broker_order_id: None,
            created_at: now,
            updated_at: now,
            deleted_at: None,
            unit_price: dec!(10.0),
            trading_vehicle_id: Uuid::new_v4(),
            action: OrderAction::Buy,
            category: OrderCategory::Market,
            status: OrderStatus::New,
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

    #[test]
    fn test_from_str_new() {
        assert_eq!("new".parse::<OrderStatus>(), Ok(OrderStatus::New));
    }

    #[test]
    fn test_from_str_replaced() {
        assert_eq!("replaced".parse::<OrderStatus>(), Ok(OrderStatus::Replaced));
    }

    #[test]
    fn test_from_str_partially_filled() {
        assert_eq!(
            "partially_filled".parse::<OrderStatus>(),
            Ok(OrderStatus::PartiallyFilled)
        );
    }

    #[test]
    fn test_from_str_filled() {
        assert_eq!("filled".parse::<OrderStatus>(), Ok(OrderStatus::Filled));
    }

    #[test]
    fn test_from_str_done_for_day() {
        assert_eq!(
            "done_for_day".parse::<OrderStatus>(),
            Ok(OrderStatus::DoneForDay)
        );
    }

    #[test]
    fn test_from_str_canceled() {
        assert_eq!("canceled".parse::<OrderStatus>(), Ok(OrderStatus::Canceled));
    }

    #[test]
    fn test_from_str_expired() {
        assert_eq!("expired".parse::<OrderStatus>(), Ok(OrderStatus::Expired));
    }

    #[test]
    fn test_from_str_accepted() {
        assert_eq!("accepted".parse::<OrderStatus>(), Ok(OrderStatus::Accepted));
    }

    #[test]
    fn test_from_str_pending_new() {
        assert_eq!(
            "pending_new".parse::<OrderStatus>(),
            Ok(OrderStatus::PendingNew)
        );
    }

    #[test]
    fn test_from_str_accepted_for_bidding() {
        assert_eq!(
            "accepted_for_bidding".parse::<OrderStatus>(),
            Ok(OrderStatus::AcceptedForBidding)
        );
    }

    #[test]
    fn test_from_str_pending_cancel() {
        assert_eq!(
            "pending_cancel".parse::<OrderStatus>(),
            Ok(OrderStatus::PendingCancel)
        );
    }

    #[test]
    fn test_from_str_pending_replace() {
        assert_eq!(
            "pending_replace".parse::<OrderStatus>(),
            Ok(OrderStatus::PendingReplace)
        );
    }

    #[test]
    fn test_from_str_stopped() {
        assert_eq!("stopped".parse::<OrderStatus>(), Ok(OrderStatus::Stopped));
    }

    #[test]
    fn test_from_str_rejected() {
        assert_eq!("rejected".parse::<OrderStatus>(), Ok(OrderStatus::Rejected));
    }

    #[test]
    fn test_from_str_suspended() {
        assert_eq!(
            "suspended".parse::<OrderStatus>(),
            Ok(OrderStatus::Suspended)
        );
    }

    #[test]
    fn test_from_str_calculated() {
        assert_eq!(
            "calculated".parse::<OrderStatus>(),
            Ok(OrderStatus::Calculated)
        );
    }

    #[test]
    fn test_from_str_held() {
        assert_eq!("held".parse::<OrderStatus>(), Ok(OrderStatus::Held));
    }

    #[test]
    fn test_from_str_unknown() {
        assert_eq!("unknown".parse::<OrderStatus>(), Ok(OrderStatus::Unknown));
    }

    #[test]
    fn test_from_str_invalid_input() {
        assert_eq!("invalid".parse::<OrderStatus>(), Err(OrderStatusParseError));
    }
}
