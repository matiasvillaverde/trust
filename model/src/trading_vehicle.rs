use chrono::NaiveDateTime;
use chrono::Utc;
use uuid::Uuid;

/// TradingVehicle entity. Like a Stock, Crypto, Fiat, Future, etc.
#[derive(PartialEq, Debug, Clone)]
pub struct TradingVehicle {
    /// Unique identifier for the trading vehicle
    pub id: Uuid,

    // Entity timestamps
    /// Timestamp when the trading vehicle was created
    pub created_at: NaiveDateTime,
    /// Timestamp when the trading vehicle was last updated
    pub updated_at: NaiveDateTime,
    /// Optional timestamp when the trading vehicle was soft-deleted
    pub deleted_at: Option<NaiveDateTime>,

    // Entity fields
    /// The symbol of the trading vehicle like BTC, ETH, AAPL, TSLA, etc.
    pub symbol: String,

    /// Optional ISIN of the trading vehicle.
    ///
    /// Note: some brokers (e.g., Alpaca assets endpoint) do not provide ISIN.
    pub isin: Option<String>,

    /// The category of the trading vehicle - crypto, fiat, stock, future, etc.
    pub category: TradingVehicleCategory,

    /// The broker that is used to trade the trading vehicle. For example: Coinbase, Binance, NASDAQ etc.
    pub broker: String,

    /// Broker-provided asset identifier (e.g., Alpaca `asset.id` UUID).
    pub broker_asset_id: Option<String>,
    /// Broker exchange code (e.g., NASDAQ, NYSE) when available.
    pub exchange: Option<String>,
    /// Broker asset class string (e.g., us_equity, crypto) when available.
    pub broker_asset_class: Option<String>,
    /// Broker asset status string (e.g., active, inactive) when available.
    pub broker_asset_status: Option<String>,

    /// Broker capability flags. Optional because not all brokers provide them.
    /// Whether the asset is tradable.
    pub tradable: Option<bool>,
    /// Whether margin trading is supported.
    pub marginable: Option<bool>,
    /// Whether short selling is supported.
    pub shortable: Option<bool>,
    /// Whether the broker flags the asset as easy to borrow.
    pub easy_to_borrow: Option<bool>,
    /// Whether fractional trading is supported.
    pub fractionable: Option<bool>,
}

/// TradingVehicleCategory enum - represents the type of the trading vehicle
#[derive(PartialEq, Debug, Clone, Copy)]
#[non_exhaustive] // This enum may be extended in the future
pub enum TradingVehicleCategory {
    /// Cryptocurrency like BTC, ETH, etc.
    Crypto,

    /// Fiat currency like USD, EUR, etc.
    Fiat,

    /// Stock like AAPL, TSLA, etc.
    Stock,
}

impl TradingVehicleCategory {
    /// Returns all available trading vehicle categories
    pub fn all() -> Vec<TradingVehicleCategory> {
        vec![
            TradingVehicleCategory::Crypto,
            TradingVehicleCategory::Fiat,
            TradingVehicleCategory::Stock,
        ]
    }
}

// Implementations

/// Error type for parsing trading vehicle category from string
#[derive(PartialEq, Debug)]
pub struct TradingVehicleCategoryParseError;

impl std::str::FromStr for TradingVehicleCategory {
    type Err = TradingVehicleCategoryParseError;
    fn from_str(category: &str) -> Result<Self, Self::Err> {
        match category {
            "crypto" => Ok(TradingVehicleCategory::Crypto),
            "fiat" => Ok(TradingVehicleCategory::Fiat),
            "stock" => Ok(TradingVehicleCategory::Stock),
            _ => Err(TradingVehicleCategoryParseError),
        }
    }
}

impl std::fmt::Display for TradingVehicleCategory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match *self {
            TradingVehicleCategory::Crypto => write!(f, "crypto"),
            TradingVehicleCategory::Fiat => write!(f, "fiat"),
            TradingVehicleCategory::Stock => write!(f, "stock"),
        }
    }
}

impl std::fmt::Display for TradingVehicle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let isin = self.isin.as_deref().unwrap_or("-");
        write!(
            f,
            "{}: {} traded in {} with ISIN: {}",
            self.symbol.to_uppercase(),
            self.category,
            self.broker.to_uppercase(),
            isin.to_uppercase(),
        )
    }
}

impl Default for TradingVehicle {
    fn default() -> Self {
        let now = Utc::now().naive_utc();
        TradingVehicle {
            id: Uuid::new_v4(),
            created_at: now,
            updated_at: now,
            deleted_at: None,
            symbol: "AAPL".to_string(),
            isin: Some("AAPL".to_string()),
            category: TradingVehicleCategory::Stock,
            broker: "NASDAQ".to_string(),
            broker_asset_id: None,
            exchange: None,
            broker_asset_class: None,
            broker_asset_status: None,
            tradable: None,
            marginable: None,
            shortable: None,
            easy_to_borrow: None,
            fractionable: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    #[test]
    fn test_trading_vehicle_from_string() {
        let result = TradingVehicleCategory::from_str("crypto")
            .expect("Failed to parse TradingVehicleCategory from string");
        assert_eq!(result, TradingVehicleCategory::Crypto);
        let result = TradingVehicleCategory::from_str("fiat")
            .expect("Failed to parse TradingVehicleCategory from string");
        assert_eq!(result, TradingVehicleCategory::Fiat);
        let result = TradingVehicleCategory::from_str("stock")
            .expect("Failed to parse TradingVehicleCategory from string");
        assert_eq!(result, TradingVehicleCategory::Stock);
    }

    #[test]
    fn test_trading_vehicle_from_invalid_string() {
        TradingVehicleCategory::from_str("FOO")
            .expect_err("Created a TradingVehicleCategory from an invalid string");
    }
}
