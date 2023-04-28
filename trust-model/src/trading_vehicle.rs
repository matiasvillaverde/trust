use chrono::NaiveDateTime;
use uuid::Uuid;

/// TradingVehicle entity. Like a Stock, Crypto, Fiat, Future, etc.
#[derive(PartialEq, Debug)]
pub struct TradingVehicle {
    pub id: Uuid,

    // Entity timestamps
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
    pub deleted_at: Option<NaiveDateTime>,

    // Entity fields
    /// The symbol of the trading vehicle like BTC, ETH, AAPL, TSLA, etc.
    pub symbol: String,

    /// The ISIN of the trading vehicle. More information: https://en.wikipedia.org/wiki/International_Securities_Identification_Number
    pub isin: String,

    /// The category of the trading vehicle - crypto, fiat, stock, future, etc.
    pub category: TradingVehicleCategory,

    /// The broker that is used to trade the trading vehicle. For example: Coinbase, Binance, NASDAQ etc.
    pub broker: String,
}

/// TradingVehicleCategory enum - represents the type of the trading vehicle
#[derive(PartialEq, Debug)]
#[non_exhaustive] // This enum may be extended in the future
pub enum TradingVehicleCategory {
    /// Crypto - cryptocurrency like BTC, ETH, etc.
    Crypto,

    /// Fiat - fiat currency like USD, EUR, etc.
    Fiat,

    /// Stock - stock like AAPL, TSLA, etc.
    Stock,

    /// Future - future like BTC-USD-210625, etc.
    Future,
}

// Implementations

impl TradingVehicleCategory {
    pub fn from_str(category: &str) -> TradingVehicleCategory {
        match category {
            "Crypto" => TradingVehicleCategory::Crypto,
            "Fiat" => TradingVehicleCategory::Fiat,
            "Stock" => TradingVehicleCategory::Stock,
            "Future" => TradingVehicleCategory::Future,
            _ => panic!("Unknown TradingVehicleCategory: {}", category),
        }
    }
}

impl std::fmt::Display for TradingVehicleCategory {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match *self {
            TradingVehicleCategory::Crypto => write!(f, "Crypto"),
            TradingVehicleCategory::Fiat => write!(f, "Fiat"),
            TradingVehicleCategory::Stock => write!(f, "Stock"),
            TradingVehicleCategory::Future => write!(f, "Future"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_from_string() {
        let result = TradingVehicleCategory::from_str("Crypto");
        assert_eq!(result, TradingVehicleCategory::Crypto);
        let result = TradingVehicleCategory::from_str("Fiat");
        assert_eq!(result, TradingVehicleCategory::Fiat);
        let result = TradingVehicleCategory::from_str("Stock");
        assert_eq!(result, TradingVehicleCategory::Stock);
        let result = TradingVehicleCategory::from_str("Future");
        assert_eq!(result, TradingVehicleCategory::Future);
    }
}
