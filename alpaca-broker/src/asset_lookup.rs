use crate::keys;
use apca::api::v2::asset::{Class, Get, Status, Symbol};
use apca::Client;
use model::{Account, TradingVehicleCategory};
use std::error::Error;
use std::str::FromStr;
use tokio::runtime::Runtime;

/// Broker-backed metadata for a tradable symbol.
#[derive(Debug, Clone, PartialEq)]
pub struct AssetMetadata {
    /// Canonical symbol as returned by Alpaca.
    pub symbol: String,
    /// Stable broker-specific identifier for this asset.
    pub broker_identifier: String,
    /// Asset class mapped to Trust trading vehicle category.
    pub category: TradingVehicleCategory,
    /// Exchange name from Alpaca.
    pub exchange: String,
    /// Whether the asset is tradable.
    pub tradable: bool,
    /// Whether the asset is active.
    pub is_active: bool,
    /// Marginability flag.
    pub marginable: bool,
    /// Shortability flag.
    pub shortable: bool,
    /// Easy-to-borrow flag.
    pub easy_to_borrow: bool,
    /// Fractionability flag.
    pub fractionable: bool,
}

pub fn fetch_asset_metadata(
    account: &Account,
    symbol: &str,
) -> Result<AssetMetadata, Box<dyn Error>> {
    let normalized_symbol = symbol.trim().to_uppercase();
    if normalized_symbol.is_empty() {
        return Err("Symbol cannot be empty".into());
    }

    let parsed_symbol = Symbol::from_str(&normalized_symbol)
        .map_err(|error| format!("Invalid symbol '{normalized_symbol}': {error}"))?;

    let api_info = keys::read_api_key(&account.environment, account)?;
    let client = Client::new(api_info);

    let asset = Runtime::new()
        .map_err(|e| Box::new(e) as Box<dyn Error>)?
        .block_on(client.issue::<Get>(&parsed_symbol))
        .map_err(|error| format!("Failed to fetch asset metadata from Alpaca: {error}"))?;

    from_asset(&asset)
}

fn from_asset(asset: &apca::api::v2::asset::Asset) -> Result<AssetMetadata, Box<dyn Error>> {
    let category = map_category(asset.class)?;
    Ok(AssetMetadata {
        symbol: asset.symbol.clone(),
        broker_identifier: asset.id.0.to_string(),
        category,
        exchange: asset.exchange.as_ref().to_string(),
        tradable: asset.tradable,
        is_active: asset.status == Status::Active,
        marginable: asset.marginable,
        shortable: asset.shortable,
        easy_to_borrow: asset.easy_to_borrow,
        fractionable: asset.fractionable,
    })
}

fn map_category(class: Class) -> Result<TradingVehicleCategory, Box<dyn Error>> {
    match class {
        Class::UsEquity => Ok(TradingVehicleCategory::Stock),
        Class::Crypto => Ok(TradingVehicleCategory::Crypto),
        Class::Unknown => Err("Unsupported Alpaca asset class: unknown".into()),
        _ => Err("Unsupported Alpaca asset class".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use apca::api::v2::asset::Asset;
    use serde_json::from_str as from_json;

    #[test]
    fn test_from_asset_maps_us_equity() {
        let raw = r#"{
            "id": "904837e3-3b76-47ec-b432-046db621571b",
            "class": "us_equity",
            "exchange": "NASDAQ",
            "symbol": "AAPL",
            "status": "active",
            "tradable": true,
            "marginable": true,
            "shortable": true,
            "easy_to_borrow": true,
            "fractionable": true
        }"#;

        let asset: Asset = from_json(raw).expect("valid asset json");
        let metadata = from_asset(&asset).expect("metadata mapping");

        assert_eq!(metadata.symbol, "AAPL");
        assert_eq!(metadata.category, TradingVehicleCategory::Stock);
        assert_eq!(metadata.exchange, "NASDAQ");
        assert!(metadata.tradable);
        assert!(metadata.is_active);
        assert_eq!(
            metadata.broker_identifier,
            "904837e3-3b76-47ec-b432-046db621571b"
        );
    }

    #[test]
    fn test_from_asset_maps_crypto() {
        let raw = r#"{
            "id": "e3cc0f27-5f4d-4b76-a3cc-9b0e5c11c8fb",
            "class": "crypto",
            "exchange": "OTC",
            "symbol": "ETH",
            "status": "active",
            "tradable": true,
            "marginable": false,
            "shortable": false,
            "easy_to_borrow": false,
            "fractionable": true
        }"#;

        let asset: Asset = from_json(raw).expect("valid crypto asset json");
        let metadata = from_asset(&asset).expect("metadata mapping");
        assert_eq!(metadata.category, TradingVehicleCategory::Crypto);
    }
}
