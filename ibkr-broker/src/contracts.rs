use crate::client::IbkrClient;
use crate::parsing::string_field_optional;
use model::{TradingVehicle, TradingVehicleCategory};
use serde_json::Value;
use std::error::Error;

#[derive(Debug, Clone, PartialEq, Eq)]
/// Minimal contract metadata returned by IBKR symbol lookup.
pub struct ContractMetadata {
    /// Broker contract identifier.
    pub conid: String,
    /// Trading symbol.
    pub symbol: String,
    /// Trust trading vehicle category used for lookup.
    pub category: TradingVehicleCategory,
    /// IBKR security type used for the contract.
    pub sec_type: String,
    /// Optional company name from IBKR.
    pub company_name: Option<String>,
    /// Optional exchange/description field from IBKR.
    pub description: Option<String>,
    /// Optional currency when IBKR returns it.
    pub currency: Option<String>,
    /// Optional exchange when IBKR returns it.
    pub exchange: Option<String>,
}

pub(crate) fn fetch_contract_metadata_with_client(
    client: &IbkrClient,
    symbol: &str,
    category: TradingVehicleCategory,
) -> Result<ContractMetadata, Box<dyn Error>> {
    let sec_type = sec_type_for_category(category)?;
    let response = client.get_json_value(
        "/iserver/secdef/search",
        &[
            ("symbol", symbol.to_uppercase()),
            ("secType", sec_type.to_string()),
        ],
    )?;
    parse_contract_metadata(&response, symbol, category)
}

pub(crate) fn parse_contract_metadata(
    response: &Value,
    symbol: &str,
    category: TradingVehicleCategory,
) -> Result<ContractMetadata, Box<dyn Error>> {
    let matches = response
        .as_array()
        .ok_or("IBKR contract search response was not an array")?;
    let target_symbol = symbol.to_ascii_uppercase();
    let contract = matches
        .iter()
        .find(|item| {
            string_field_optional(item, "symbol")
                .map(|value| value.eq_ignore_ascii_case(&target_symbol))
                .unwrap_or(false)
        })
        .ok_or_else(|| format!("IBKR contract search returned no exact match for '{symbol}'"))?;

    Ok(ContractMetadata {
        conid: string_field_optional(contract, "conid")
            .ok_or("IBKR contract match did not include conid")?,
        symbol: string_field_optional(contract, "symbol").unwrap_or_else(|| symbol.to_uppercase()),
        category,
        sec_type: string_field_optional(contract, "secType")
            .or_else(|| string_field_optional(contract, "sectype"))
            .unwrap_or_else(|| sec_type_for_category(category).unwrap_or("STK").to_string()),
        company_name: string_field_optional(contract, "companyName"),
        description: string_field_optional(contract, "description"),
        currency: string_field_optional(contract, "currency"),
        exchange: string_field_optional(contract, "exchange"),
    })
}

pub(crate) fn resolve_conid(
    client: &IbkrClient,
    vehicle: &TradingVehicle,
) -> Result<String, Box<dyn Error>> {
    if let Some(conid) = vehicle
        .broker_asset_id
        .as_ref()
        .filter(|value| !value.trim().is_empty())
    {
        return Ok(conid.to_string());
    }
    Ok(fetch_contract_metadata_with_client(client, &vehicle.symbol, vehicle.category)?.conid)
}

pub(crate) fn sec_type_for_vehicle(
    vehicle: &TradingVehicle,
) -> Result<&'static str, Box<dyn Error>> {
    sec_type_for_category(vehicle.category)
}

pub(crate) fn sec_type_for_category(
    category: TradingVehicleCategory,
) -> Result<&'static str, Box<dyn Error>> {
    match category {
        TradingVehicleCategory::Stock | TradingVehicleCategory::Etf => Ok("STK"),
        TradingVehicleCategory::Bond => Ok("BOND"),
        TradingVehicleCategory::Crypto => Ok("CRYPTO"),
        TradingVehicleCategory::Fiat => Ok("CASH"),
        _ => Err("IBKR broker does not support this trading vehicle category".into()),
    }
}

pub(crate) fn listing_exchange(vehicle: &TradingVehicle) -> String {
    vehicle
        .exchange
        .as_ref()
        .filter(|value| !value.trim().is_empty())
        .map(|value| value.to_string())
        .unwrap_or_else(|| "SMART".to_string())
}

#[cfg(test)]
mod tests {
    use super::{listing_exchange, parse_contract_metadata, sec_type_for_category};
    use model::{TradingVehicle, TradingVehicleCategory};
    use serde_json::json;

    #[test]
    fn contract_search_prefers_exact_symbol_match() {
        let payload = json!([
            { "conid": "1", "symbol": "AAPL1", "exchange": "TEST" },
            { "conid": "2", "symbol": "AAPL", "exchange": "SMART" }
        ]);

        let metadata = parse_contract_metadata(&payload, "aapl", TradingVehicleCategory::Stock)
            .expect("metadata");

        assert_eq!(metadata.conid, "2");
        assert_eq!(metadata.symbol, "AAPL");
        assert_eq!(metadata.category, TradingVehicleCategory::Stock);
        assert_eq!(metadata.sec_type, "STK");
        assert_eq!(metadata.exchange.as_deref(), Some("SMART"));
    }

    #[test]
    fn contract_search_rejects_symbol_mismatch() {
        let payload = json!([{ "conid": "1", "symbol": "MSFT", "exchange": "SMART" }]);

        let error = parse_contract_metadata(&payload, "unknown", TradingVehicleCategory::Etf)
            .expect_err("symbol mismatch");

        assert!(error.to_string().contains("no exact match"));
    }

    #[test]
    fn sec_type_mapping_covers_multi_asset_categories() {
        assert_eq!(
            sec_type_for_category(TradingVehicleCategory::Stock).expect("stock"),
            "STK"
        );
        assert_eq!(
            sec_type_for_category(TradingVehicleCategory::Etf).expect("etf"),
            "STK"
        );
        assert_eq!(
            sec_type_for_category(TradingVehicleCategory::Bond).expect("bond"),
            "BOND"
        );
        assert_eq!(
            sec_type_for_category(TradingVehicleCategory::Crypto).expect("crypto"),
            "CRYPTO"
        );
        assert_eq!(
            sec_type_for_category(TradingVehicleCategory::Fiat).expect("fiat"),
            "CASH"
        );
    }

    #[test]
    fn listing_exchange_defaults_to_smart_and_preserves_explicit_exchange() {
        let default_exchange = TradingVehicle {
            exchange: None,
            ..TradingVehicle::default()
        };
        let explicit_exchange = TradingVehicle {
            exchange: Some("ARCA".to_string()),
            ..TradingVehicle::default()
        };

        assert_eq!(listing_exchange(&default_exchange), "SMART");
        assert_eq!(listing_exchange(&explicit_exchange), "ARCA");
    }
}
