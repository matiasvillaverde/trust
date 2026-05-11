use crate::currency::Currency;
use crate::order::Order;
use crate::trading_vehicle::{TradingVehicle, TradingVehicleCategory};
use chrono::Utc;
use chrono::{Datelike, Days, NaiveDate, NaiveDateTime, Weekday};
use rust_decimal::Decimal;
use uuid::Uuid;

/// Trade entity - represents a single trade.
/// Trade is the most important entity of the trust model.
#[derive(PartialEq, Debug, Clone)]
pub struct Trade {
    /// Unique identifier for the trade
    pub id: Uuid,

    // Entity timestamps
    /// Timestamp when the trade was created
    pub created_at: NaiveDateTime,
    /// Timestamp when the trade was last updated
    pub updated_at: NaiveDateTime,
    /// Timestamp when the trade was deleted (soft delete)
    pub deleted_at: Option<NaiveDateTime>,

    // Entity fields
    /// The trading vehicle that the trade is associated with. For example, TSLA, AAPL, BTC, etc.
    pub trading_vehicle: TradingVehicle,

    /// The category of the trade - long or short
    pub category: TradeCategory,

    /// The status of the trade. Reflecting the lifecycle of the trade and its internal orders.
    pub status: Status,

    /// The currency of the trade
    pub currency: Currency,

    /// Expected settlement date for the trade, based on the asset category and trade date.
    pub settlement_date: Option<NaiveDate>,

    /// The safety stop - the order that is used to protect the trade from losing too much money.
    /// The safety stop is an order that is used to close the trade if the price goes in the wrong direction.
    /// The safety stop must be of type market order to get out of the trade as soon as possible.
    pub safety_stop: Order,

    /// The entry orders - the orders that are used to enter the trade.
    /// The entry orders must be of type limit order to get the best price.
    pub entry: Order,

    /// The exit targets orders - the orders that are used to exit the trade.
    /// It is a take_profit order that is used to close the trade with a profit.
    pub target: Order,

    /// The account that the trade is associated with
    pub account_id: Uuid,

    /// The balance of the trade - It is a cache of the calculations of the trade.
    /// It is a snapshot of the trade. It should be updated every time the trade is updated.
    /// WARNING: It is read-only and it can be out of sync if the trade is open.
    pub balance: TradeBalance,

    // Metadata fields for trade hypothesis and analytics
    /// Trade thesis - reasoning behind the trade (max 200 chars)
    pub thesis: Option<String>,

    /// Market sector (e.g., technology, healthcare, finance)
    pub sector: Option<String>,

    /// Asset class (e.g., stocks, options, futures, crypto)
    pub asset_class: Option<String>,

    /// Trading context (e.g., Elliott Wave count, S/R levels, indicators)
    pub context: Option<String>,
}

impl Trade {
    /// Returns the default settlement date for a trade category using common settlement conventions.
    pub fn default_settlement_date_for_category(
        category: TradingVehicleCategory,
        trade_date: NaiveDate,
    ) -> NaiveDate {
        match category {
            TradingVehicleCategory::Crypto | TradingVehicleCategory::Fiat => trade_date,
            TradingVehicleCategory::Stock | TradingVehicleCategory::Etf => {
                Self::add_business_days(trade_date, 1)
            }
            TradingVehicleCategory::Bond => Self::add_business_days(trade_date, 2),
        }
    }

    fn add_business_days(mut date: NaiveDate, days: u8) -> NaiveDate {
        let mut remaining = days;
        while remaining > 0 {
            let Some(next_date) = date.checked_add_days(Days::new(1)) else {
                return date;
            };
            date = next_date;
            if !matches!(date.weekday(), Weekday::Sat | Weekday::Sun) {
                remaining = remaining.saturating_sub(1);
            }
        }
        date
    }
}

impl std::fmt::Display for Trade {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}: quantity: {}, category: {}, currency: {}, safety_stop: {}, entry: {}, target: {}, status: {}",
            self.trading_vehicle.symbol,
            self.safety_stop.quantity,
            self.category,
            self.currency,
            self.safety_stop.unit_price,
            self.entry.unit_price,
            self.target.unit_price,
            self.status,
        )
    }
}

/// The status an order can have.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Default)]
pub enum Status {
    /// The trade has been created and waiting for
    /// funding. This is the usual initial state of trade.
    #[default]
    New,
    /// The trade has been funded and it is ready to be submitted.
    Funded,
    /// The trade has been submitted to the broker.
    Submitted,
    /// The trade has been partially filled.
    PartiallyFilled,
    /// The trade has been completely filled.
    Filled,
    /// The trade has been closed by the broker in the stop.
    ClosedStopLoss,
    /// The trade has been closed by the broker in the target.
    ClosedTarget,
    /// The trade has been canceled by the user or the broker.
    Canceled,
    /// The trade has been expired.
    Expired,
    /// The trade has been rejected by the broker or internal rules.
    Rejected,
}

impl Status {
    /// Returns all possible trade status variants
    pub fn all() -> Vec<Status> {
        vec![
            Status::New,
            Status::Funded,
            Status::Submitted,
            Status::PartiallyFilled,
            Status::Filled,
            Status::ClosedStopLoss,
            Status::ClosedTarget,
            Status::Canceled,
            Status::Expired,
            Status::Rejected,
        ]
    }
}

impl std::fmt::Display for Status {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let status = match self {
            Status::New => "new",
            Status::Funded => "funded",
            Status::Submitted => "submitted",
            Status::PartiallyFilled => "partially_filled",
            Status::Filled => "filled",
            Status::Canceled => "canceled",
            Status::Expired => "expired",
            Status::Rejected => "rejected",
            Status::ClosedStopLoss => "closed_stop_loss",
            Status::ClosedTarget => "closed_target",
        };
        write!(f, "{status}")
    }
}

/// Error returned when parsing an invalid trade status string
#[derive(Debug)]
pub struct TradeStatusParseError;
impl std::str::FromStr for Status {
    type Err = TradeStatusParseError;
    fn from_str(status: &str) -> Result<Self, Self::Err> {
        match status {
            "new" => Ok(Status::New),
            "funded" => Ok(Status::Funded),
            "submitted" => Ok(Status::Submitted),
            "partially_filled" => Ok(Status::PartiallyFilled),
            "filled" => Ok(Status::Filled),
            "canceled" => Ok(Status::Canceled),
            "expired" => Ok(Status::Expired),
            "rejected" => Ok(Status::Rejected),
            "closed_stop_loss" => Ok(Status::ClosedStopLoss),
            "closed_target" => Ok(Status::ClosedTarget),
            _ => Err(TradeStatusParseError),
        }
    }
}

/// The category of the trade - Being a bull or a bear
#[derive(PartialEq, Debug, Clone, Copy, Default)]
pub enum TradeCategory {
    /// Long trade - Bull - buy an asset and sell it later at a higher price
    #[default]
    Long,
    /// Short trade - Bear - sell an asset and buy it later at a lower price
    Short,
}

impl TradeCategory {
    /// Returns all possible trade category variants
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
/// Error returned when parsing an invalid trade category string
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

/// Trade balance entity - represents the financial snapshot of a trade
#[derive(PartialEq, Debug, Clone)]
pub struct TradeBalance {
    /// Unique identifier for the trade balance
    pub id: Uuid,

    // Entity timestamps
    /// Timestamp when the trade balance was created
    pub created_at: NaiveDateTime,
    /// Timestamp when the trade balance was last updated
    pub updated_at: NaiveDateTime,
    /// Timestamp when the trade balance was deleted (soft delete)
    pub deleted_at: Option<NaiveDateTime>,

    /// The currency of the trade
    pub currency: Currency,

    /// Total amount of money that was used to open the trade
    pub funding: Decimal,

    /// Total amount of money currently in the market (the amount of money that is currently invested)
    pub capital_in_market: Decimal,

    /// Total amount of money available
    pub capital_out_market: Decimal,

    /// Total amount of money that it must be paid out to the tax authorities
    pub taxed: Decimal,

    /// Total amount of money that we have earned or lost from the trade
    pub total_performance: Decimal,
}

impl Default for Trade {
    fn default() -> Self {
        let now = Utc::now().naive_utc();
        Trade {
            id: Uuid::new_v4(),
            created_at: now,
            updated_at: now,
            deleted_at: None,
            status: Status::default(),
            category: TradeCategory::default(),
            currency: Currency::default(),
            settlement_date: None,
            trading_vehicle: TradingVehicle::default(),
            safety_stop: Order::default(),
            entry: Order::default(),
            target: Order::default(),
            account_id: Uuid::new_v4(),
            balance: TradeBalance::default(),
            thesis: None,
            sector: None,
            asset_class: None,
            context: None,
        }
    }
}

impl Default for TradeBalance {
    fn default() -> Self {
        let now = Utc::now().naive_utc();
        TradeBalance {
            id: Uuid::new_v4(),
            created_at: now,
            updated_at: now,
            deleted_at: None,
            currency: Currency::default(),
            funding: Decimal::default(),
            capital_in_market: Decimal::default(),
            capital_out_market: Decimal::default(),
            taxed: Decimal::default(),
            total_performance: Decimal::default(),
        }
    }
}

/// Lightweight view used by analytics (for example leveling snapshots) to avoid
/// materializing full Trade graphs (orders/vehicles) when only performance is needed.
#[derive(PartialEq, Debug, Clone)]
pub struct ClosedTradePerformance {
    /// Closed trade identifier.
    pub trade_id: Uuid,
    /// Persisted total performance for this trade.
    pub total_performance: Decimal,
}

#[cfg(test)]
mod tests {
    use super::{Status, Trade, TradeBalance, TradeCategory};
    use crate::{Currency, Order, TradingVehicle, TradingVehicleCategory};
    use chrono::NaiveDate;
    use rust_decimal_macros::dec;
    use std::str::FromStr;

    #[test]
    fn status_display_parse_and_all_variants_are_stable() {
        let cases = [
            (Status::New, "new"),
            (Status::Funded, "funded"),
            (Status::Submitted, "submitted"),
            (Status::PartiallyFilled, "partially_filled"),
            (Status::Filled, "filled"),
            (Status::ClosedStopLoss, "closed_stop_loss"),
            (Status::ClosedTarget, "closed_target"),
            (Status::Canceled, "canceled"),
            (Status::Expired, "expired"),
            (Status::Rejected, "rejected"),
        ];

        assert_eq!(
            Status::all(),
            cases.iter().map(|(status, _)| *status).collect::<Vec<_>>()
        );
        assert_eq!(Status::default(), Status::New);
        for (status, text) in cases {
            assert_eq!(status.to_string(), text);
            assert_eq!(Status::from_str(text).expect("status should parse"), status);
        }
        assert!(Status::from_str("closed").is_err());
    }

    #[test]
    fn trade_category_display_parse_and_all_variants_are_stable() {
        let cases = [
            (TradeCategory::Long, "long"),
            (TradeCategory::Short, "short"),
        ];

        assert_eq!(
            TradeCategory::all(),
            cases
                .iter()
                .map(|(category, _)| *category)
                .collect::<Vec<_>>()
        );
        assert_eq!(TradeCategory::default(), TradeCategory::Long);
        for (category, text) in cases {
            assert_eq!(category.to_string(), text);
            assert_eq!(
                TradeCategory::from_str(text).expect("category should parse"),
                category
            );
        }
        assert!(TradeCategory::from_str("flat").is_err());
    }

    #[test]
    fn trade_default_sets_new_long_trade_with_empty_metadata() {
        let trade = Trade::default();

        assert_eq!(trade.created_at, trade.updated_at);
        assert!(trade.deleted_at.is_none());
        assert_eq!(trade.status, Status::New);
        assert_eq!(trade.category, TradeCategory::Long);
        assert_eq!(trade.currency, Currency::default());
        assert!(trade.settlement_date.is_none());
        assert!(trade.thesis.is_none());
        assert!(trade.sector.is_none());
        assert!(trade.asset_class.is_none());
        assert!(trade.context.is_none());
    }

    #[test]
    fn default_settlement_date_uses_asset_category_conventions() {
        let friday = NaiveDate::from_ymd_opt(2026, 5, 15).expect("valid date");

        assert_eq!(
            Trade::default_settlement_date_for_category(TradingVehicleCategory::Crypto, friday),
            friday
        );
        assert_eq!(
            Trade::default_settlement_date_for_category(TradingVehicleCategory::Fiat, friday),
            friday
        );
        assert_eq!(
            Trade::default_settlement_date_for_category(TradingVehicleCategory::Stock, friday),
            NaiveDate::from_ymd_opt(2026, 5, 18).expect("valid date")
        );
        assert_eq!(
            Trade::default_settlement_date_for_category(TradingVehicleCategory::Etf, friday),
            NaiveDate::from_ymd_opt(2026, 5, 18).expect("valid date")
        );
        assert_eq!(
            Trade::default_settlement_date_for_category(TradingVehicleCategory::Bond, friday),
            NaiveDate::from_ymd_opt(2026, 5, 19).expect("valid date")
        );
    }

    #[test]
    fn trade_balance_default_sets_zero_currency_snapshot() {
        let balance = TradeBalance::default();

        assert_eq!(balance.created_at, balance.updated_at);
        assert!(balance.deleted_at.is_none());
        assert_eq!(balance.currency, Currency::default());
        assert_eq!(balance.funding, dec!(0));
        assert_eq!(balance.capital_in_market, dec!(0));
        assert_eq!(balance.capital_out_market, dec!(0));
        assert_eq!(balance.taxed, dec!(0));
        assert_eq!(balance.total_performance, dec!(0));
    }

    #[test]
    fn trade_display_includes_core_trading_terms() {
        let trade = Trade {
            trading_vehicle: TradingVehicle {
                symbol: "AAPL".to_string(),
                ..Default::default()
            },
            category: TradeCategory::Short,
            status: Status::Funded,
            currency: Currency::USD,
            safety_stop: Order {
                quantity: dec!(7),
                unit_price: dec!(95),
                ..Default::default()
            },
            entry: Order {
                unit_price: dec!(100),
                ..Default::default()
            },
            target: Order {
                unit_price: dec!(115),
                ..Default::default()
            },
            ..Default::default()
        };

        assert_eq!(
            trade.to_string(),
            "AAPL: quantity: 7, category: short, currency: USD, safety_stop: 95, entry: 100, target: 115, status: funded"
        );
    }
}
