use crate::currency::Currency;
use crate::order::Order;
use crate::price::Price;
use crate::target::Target;
use crate::trading_vehicle::TradingVehicle;
use chrono::NaiveDateTime;
use uuid::Uuid;

/// Trade entity - represents a single trade.
/// Trade is the most important entity of the trust model.
#[derive(PartialEq, Debug, Clone)]
pub struct Trade {
    pub id: Uuid,

    // Entity timestamps
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
    pub deleted_at: Option<NaiveDateTime>,

    // Entity fields
    /// The trading vehicle that the trade is associated with. For example, TSLA, AAPL, BTC, etc.
    pub trading_vehicle: TradingVehicle,

    /// The category of the trade - long or short
    pub category: TradeCategory,

    /// The currency of the trade
    pub currency: Currency,

    /// The safety stop - the order that is used to protect the trade from losing too much money.
    /// The safety stop is an order that is used to close the trade if the price goes in the wrong direction.
    /// The safety stop must be of type market order to get out of the trade as soon as possible.
    pub safety_stop: Order,

    /// The entry orders - the orders that are used to enter the trade.
    /// The entry orders must be of type limit order to get the best price.
    pub entry: Order,

    /// The exit targets orders - the orders that are used to exit the trade.
    /// The exit targets orders should be of type limit order to get the best price.
    /// The exit targets orders can be used to secure part of the profit.
    pub exit_targets: Vec<Target>,

    /// The account that the trade is associated with
    pub account_id: Uuid,

    /// When the trade was approved by applying the rules of the account
    pub approved_at: Option<NaiveDateTime>,

    /// When the trade was rejected by applying the rules of the account
    pub rejected_at: Option<NaiveDateTime>,

    /// When the trade started to be executed by the broker
    pub opened_at: Option<NaiveDateTime>,

    /// When the trade failed to be executed by the broker
    pub failed_at: Option<NaiveDateTime>,

    /// When the trade was closed by the broker. All their orders were executed.
    pub closed_at: Option<NaiveDateTime>,

    /// The rule that rejected the trade. It has to be a rule of type error.
    pub rejected_by_rule_id: Option<Uuid>,

    /// The overview of the trade - It is a cache of the calculations of the trade.
    /// It is a snapshot of the trade. It should be updated every time the trade is updated.
    /// WARNING: It is read-only and it can be out of sync if the trade is open.
    pub overview: TradeOverview,
}

impl std::fmt::Display for Trade {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}: quantity: {}, category: {}, currency: {}, safety_stop: {}, entry: {}, exit_targets: {}",
            self.trading_vehicle.symbol,
            self.safety_stop.quantity,
            self.category,
            self.currency,
            self.safety_stop.unit_price.amount,
            self.entry.unit_price.amount,
            self.exit_targets.len(),
        )
    }
}

/// The category of the trade - Being a bull or a bear
#[derive(PartialEq, Debug, Clone, Copy)]
pub enum TradeCategory {
    /// Long trade - Bull - buy an asset and sell it later at a higher price
    Long,

    /// Short trade - Bear - sell an asset and buy it later at a lower price
    Short,
}

impl TradeCategory {
    pub fn all() -> Vec<TradeCategory> {
        vec![TradeCategory::Long, TradeCategory::Short]
    }
}

impl std::fmt::Display for TradeCategory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TradeCategory::Long => write!(f, "long"),
            TradeCategory::Short => write!(f, "short"),
        }
    }
}
#[derive(Debug)]
pub struct TradeCategoryParseError;
impl std::str::FromStr for TradeCategory {
    type Err = TradeCategoryParseError;
    fn from_str(category: &str) -> Result<Self, Self::Err> {
        match category {
            "long" => Ok(TradeCategory::Long),
            "short" => Ok(TradeCategory::Short),
            _ => Err(TradeCategoryParseError),
        }
    }
}

#[derive(PartialEq, Debug, Clone)]
pub struct TradeOverview {
    pub id: Uuid,

    // Entity timestamps
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
    pub deleted_at: Option<NaiveDateTime>,

    /// Total amount of money that was used to open the trade
    pub total_input: Price,

    /// Total amount of money currently in the market (the amount of money that is currently invested)
    pub total_in_market: Price,

    /// Total amount of money available
    pub total_out_market: Price,

    /// Total amount of money that it must be paid out to the tax authorities
    pub total_taxable: Price,

    /// Total amount of money that we have earned or lost from the trade
    pub total_performance: Price,
}
