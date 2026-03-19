use alpaca_broker::{AlpacaBroker, AssetMetadata};
use ibkr_broker::{ContractMetadata, IbkrBroker};
use model::{database::TradingVehicleUpsert, Account, BrokerKind, TradingVehicleCategory};

#[derive(Debug, Clone)]
pub(crate) struct ImportedTradingVehicle {
    pub(crate) upsert: TradingVehicleUpsert,
    pub(crate) summary: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TradingVehicleImportError {
    code: &'static str,
    message: String,
}

impl TradingVehicleImportError {
    pub(crate) fn code(&self) -> &'static str {
        self.code
    }

    pub(crate) fn message(&self) -> &str {
        &self.message
    }

    fn new(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
}

pub(crate) fn import_from_broker(
    account: &Account,
    symbol: &str,
    broker_kind: BrokerKind,
) -> Result<ImportedTradingVehicle, TradingVehicleImportError> {
    match broker_kind {
        BrokerKind::Alpaca => {
            let metadata =
                AlpacaBroker::fetch_asset_metadata(account, symbol).map_err(|error| {
                    TradingVehicleImportError::new("alpaca_import_failed", format!("{error}"))
                })?;
            imported_from_alpaca(metadata)
        }
        BrokerKind::Ibkr => {
            let metadata =
                IbkrBroker::fetch_contract_metadata(account, symbol).map_err(|error| {
                    TradingVehicleImportError::new("ibkr_import_failed", format!("{error}"))
                })?;
            Ok(imported_from_ibkr(metadata))
        }
    }
}

fn imported_from_alpaca(
    metadata: AssetMetadata,
) -> Result<ImportedTradingVehicle, TradingVehicleImportError> {
    if !metadata.is_active {
        return Err(TradingVehicleImportError::new(
            "alpaca_import_unavailable",
            format!("symbol '{}' is inactive", metadata.symbol),
        ));
    }

    if !metadata.tradable {
        return Err(TradingVehicleImportError::new(
            "alpaca_import_unavailable",
            format!("symbol '{}' is not tradable", metadata.symbol),
        ));
    }

    Ok(ImportedTradingVehicle {
        upsert: TradingVehicleUpsert {
            symbol: metadata.symbol.clone(),
            isin: None,
            category: metadata.category,
            broker: "alpaca".to_string(),
            broker_asset_id: Some(metadata.broker_identifier.clone()),
            exchange: Some(metadata.exchange.clone()),
            broker_asset_class: None,
            broker_asset_status: Some(if metadata.is_active {
                "active".to_string()
            } else {
                "inactive".to_string()
            }),
            tradable: Some(metadata.tradable),
            marginable: Some(metadata.marginable),
            shortable: Some(metadata.shortable),
            easy_to_borrow: Some(metadata.easy_to_borrow),
            fractionable: Some(metadata.fractionable),
        },
        summary: format!(
            "Imported from Alpaca: symbol={}, category={}, exchange={}, tradable={}, marginable={}, shortable={}, fractionable={}, broker_id={}",
            metadata.symbol,
            metadata.category,
            metadata.exchange,
            metadata.tradable,
            metadata.marginable,
            metadata.shortable,
            metadata.fractionable,
            metadata.broker_identifier,
        ),
    })
}

fn imported_from_ibkr(metadata: ContractMetadata) -> ImportedTradingVehicle {
    let exchange = metadata.exchange.clone().or(metadata.description.clone());
    ImportedTradingVehicle {
        upsert: TradingVehicleUpsert {
            symbol: metadata.symbol.clone(),
            isin: None,
            category: TradingVehicleCategory::Stock,
            broker: "ibkr".to_string(),
            broker_asset_id: Some(metadata.conid.clone()),
            exchange,
            broker_asset_class: Some("stock".to_string()),
            broker_asset_status: None,
            tradable: None,
            marginable: None,
            shortable: None,
            easy_to_borrow: None,
            fractionable: None,
        },
        summary: format!(
            "Imported from IBKR: symbol={}, conid={}, exchange={}",
            metadata.symbol,
            metadata.conid,
            metadata.exchange.unwrap_or_else(|| "-".to_string()),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::{imported_from_alpaca, imported_from_ibkr};
    use alpaca_broker::AssetMetadata;
    use ibkr_broker::ContractMetadata;
    use model::TradingVehicleCategory;

    #[test]
    fn alpaca_import_rejects_inactive_symbols() {
        let error = imported_from_alpaca(AssetMetadata {
            symbol: "AAPL".to_string(),
            broker_identifier: "asset-1".to_string(),
            category: TradingVehicleCategory::Stock,
            exchange: "NASDAQ".to_string(),
            tradable: true,
            is_active: false,
            marginable: true,
            shortable: true,
            easy_to_borrow: true,
            fractionable: true,
        })
        .expect_err("inactive symbols should be rejected");

        assert_eq!(error.code(), "alpaca_import_unavailable");
        assert_eq!(error.message(), "symbol 'AAPL' is inactive");
    }

    #[test]
    fn ibkr_import_maps_contract_metadata_into_upsert_shape() {
        let imported = imported_from_ibkr(ContractMetadata {
            conid: "265598".to_string(),
            symbol: "AAPL".to_string(),
            company_name: Some("Apple Inc".to_string()),
            description: Some("NASDAQ".to_string()),
            currency: Some("USD".to_string()),
            exchange: Some("SMART".to_string()),
        });

        assert_eq!(imported.upsert.symbol, "AAPL");
        assert_eq!(imported.upsert.broker, "ibkr");
        assert_eq!(imported.upsert.broker_asset_id.as_deref(), Some("265598"));
        assert_eq!(imported.upsert.exchange.as_deref(), Some("SMART"));
        assert_eq!(imported.upsert.broker_asset_class.as_deref(), Some("stock"));
        assert_eq!(
            imported.summary,
            "Imported from IBKR: symbol=AAPL, conid=265598, exchange=SMART"
        );
    }
}
