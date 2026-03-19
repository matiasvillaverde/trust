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
) -> Result<ContractMetadata, Box<dyn Error>> {
    let response = client.get_json_value(
        "/iserver/secdef/search",
        &[
            ("symbol", symbol.to_uppercase()),
            ("secType", "STK".to_string()),
        ],
    )?;
    parse_contract_metadata(&response, symbol)
}

pub(crate) fn parse_contract_metadata(
    response: &Value,
    symbol: &str,
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
        .or_else(|| matches.first())
        .ok_or_else(|| format!("IBKR contract search returned no matches for '{symbol}'"))?;

    Ok(ContractMetadata {
        conid: string_field_optional(contract, "conid")
            .ok_or("IBKR contract match did not include conid")?,
        symbol: string_field_optional(contract, "symbol").unwrap_or_else(|| symbol.to_uppercase()),
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
    match vehicle.category {
        TradingVehicleCategory::Stock => {
            Ok(fetch_contract_metadata_with_client(client, &vehicle.symbol)?.conid)
        }
        _ => Err(format!(
            "IBKR broker currently supports stock contract lookup only, got '{}'",
            vehicle.category
        )
        .into()),
    }
}

pub(crate) fn sec_type_for_vehicle(
    vehicle: &TradingVehicle,
) -> Result<&'static str, Box<dyn Error>> {
    match vehicle.category {
        TradingVehicleCategory::Stock => Ok("STK"),
        _ => Err(format!(
            "IBKR broker currently supports stock orders only, got '{}'",
            vehicle.category
        )
        .into()),
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
    use super::parse_contract_metadata;
    use serde_json::json;

    #[test]
    fn contract_search_prefers_exact_symbol_match() {
        let payload = json!([
            { "conid": "1", "symbol": "AAPL1", "exchange": "TEST" },
            { "conid": "2", "symbol": "AAPL", "exchange": "SMART" }
        ]);

        let metadata = parse_contract_metadata(&payload, "aapl").expect("metadata");

        assert_eq!(metadata.conid, "2");
        assert_eq!(metadata.symbol, "AAPL");
        assert_eq!(metadata.exchange.as_deref(), Some("SMART"));
    }

    #[test]
    fn contract_search_falls_back_to_first_match() {
        let payload = json!([{ "conid": "1", "symbol": "MSFT", "exchange": "SMART" }]);

        let metadata = parse_contract_metadata(&payload, "unknown").expect("metadata");

        assert_eq!(metadata.conid, "1");
        assert_eq!(metadata.symbol, "MSFT");
    }
}
