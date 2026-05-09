use alpaca_broker::{AlpacaBroker, AssetMetadata};
use ibkr_broker::{ContractMetadata, IbkrBroker};
use model::{database::TradingVehicleUpsert, Account, BrokerKind, TradingVehicleCategory};
use std::error::Error;

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
    category_hint: Option<TradingVehicleCategory>,
) -> Result<ImportedTradingVehicle, TradingVehicleImportError> {
    import_from_broker_with_fetchers(
        account,
        symbol,
        broker_kind,
        category_hint,
        AlpacaBroker::fetch_asset_metadata,
        IbkrBroker::fetch_contract_metadata_for_category,
    )
}

fn import_from_broker_with_fetchers<FetchAlpaca, FetchIbkr>(
    account: &Account,
    symbol: &str,
    broker_kind: BrokerKind,
    category_hint: Option<TradingVehicleCategory>,
    mut fetch_alpaca: FetchAlpaca,
    mut fetch_ibkr: FetchIbkr,
) -> Result<ImportedTradingVehicle, TradingVehicleImportError>
where
    FetchAlpaca: FnMut(&Account, &str) -> Result<AssetMetadata, Box<dyn Error>>,
    FetchIbkr:
        FnMut(&Account, &str, TradingVehicleCategory) -> Result<ContractMetadata, Box<dyn Error>>,
{
    match broker_kind {
        BrokerKind::Alpaca => {
            let metadata = fetch_alpaca(account, symbol).map_err(|error| {
                TradingVehicleImportError::new("alpaca_import_failed", format!("{error}"))
            })?;
            imported_from_alpaca(metadata)
        }
        BrokerKind::Ibkr => {
            let category = category_hint.unwrap_or(TradingVehicleCategory::Stock);
            let metadata = fetch_ibkr(account, symbol, category).map_err(|error| {
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
            broker_asset_status: Some("active".to_string()),
            tradable: Some(metadata.tradable),
            marginable: Some(metadata.marginable),
            shortable: Some(metadata.shortable),
            easy_to_borrow: Some(metadata.easy_to_borrow),
            fractionable: Some(metadata.fractionable),
            fixed_income: None,
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
            category: metadata.category,
            broker: "ibkr".to_string(),
            broker_asset_id: Some(metadata.conid.clone()),
            exchange,
            broker_asset_class: Some(metadata.sec_type.to_lowercase()),
            broker_asset_status: None,
            tradable: None,
            marginable: None,
            shortable: None,
            easy_to_borrow: None,
            fractionable: None,
            fixed_income: None,
        },
        summary: format!(
            "Imported from IBKR: symbol={}, category={}, sec_type={}, conid={}, exchange={}",
            metadata.symbol,
            metadata.category,
            metadata.sec_type,
            metadata.conid,
            metadata.exchange.unwrap_or_else(|| "-".to_string()),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::{
        import_from_broker, import_from_broker_with_fetchers, imported_from_alpaca,
        imported_from_ibkr,
    };
    use alpaca_broker::AssetMetadata;
    use ibkr_broker::ContractMetadata;
    use model::{Account, BrokerKind, TradingVehicleCategory};

    fn alpaca_metadata(tradable: bool, is_active: bool) -> AssetMetadata {
        AssetMetadata {
            symbol: "AAPL".to_string(),
            broker_identifier: "asset-1".to_string(),
            category: TradingVehicleCategory::Stock,
            exchange: "NASDAQ".to_string(),
            tradable,
            is_active,
            marginable: true,
            shortable: true,
            easy_to_borrow: true,
            fractionable: true,
        }
    }

    fn ibkr_metadata(category: TradingVehicleCategory) -> ContractMetadata {
        ContractMetadata {
            conid: "265598".to_string(),
            symbol: "AAPL".to_string(),
            category,
            sec_type: "STK".to_string(),
            company_name: Some("Apple Inc".to_string()),
            description: Some("NASDAQ".to_string()),
            currency: Some("USD".to_string()),
            exchange: Some("SMART".to_string()),
        }
    }

    fn alpaca_success_fetcher(
        _account: &Account,
        symbol: &str,
    ) -> Result<AssetMetadata, Box<dyn std::error::Error>> {
        assert_eq!(symbol, "AAPL");
        Ok(alpaca_metadata(true, true))
    }

    fn ibkr_success_fetcher(
        _account: &Account,
        symbol: &str,
        category: TradingVehicleCategory,
    ) -> Result<ContractMetadata, Box<dyn std::error::Error>> {
        assert_eq!(symbol, "AAPL");
        Ok(ibkr_metadata(category))
    }

    #[test]
    fn alpaca_import_maps_active_tradable_asset_into_upsert_shape() {
        let imported =
            imported_from_alpaca(alpaca_metadata(true, true)).expect("active tradable asset maps");

        assert_eq!(imported.upsert.symbol, "AAPL");
        assert_eq!(imported.upsert.broker, "alpaca");
        assert_eq!(imported.upsert.category, TradingVehicleCategory::Stock);
        assert_eq!(imported.upsert.isin, None);
        assert_eq!(imported.upsert.broker_asset_id.as_deref(), Some("asset-1"));
        assert_eq!(imported.upsert.exchange.as_deref(), Some("NASDAQ"));
        assert_eq!(
            imported.upsert.broker_asset_status.as_deref(),
            Some("active")
        );
        assert_eq!(imported.upsert.tradable, Some(true));
        assert_eq!(imported.upsert.marginable, Some(true));
        assert_eq!(imported.upsert.shortable, Some(true));
        assert_eq!(imported.upsert.easy_to_borrow, Some(true));
        assert_eq!(imported.upsert.fractionable, Some(true));
        assert_eq!(imported.upsert.fixed_income, None);
        assert_eq!(
            imported.summary,
            "Imported from Alpaca: symbol=AAPL, category=stock, exchange=NASDAQ, tradable=true, marginable=true, shortable=true, fractionable=true, broker_id=asset-1"
        );
    }

    #[test]
    fn alpaca_import_rejects_inactive_symbols() {
        let error = imported_from_alpaca(alpaca_metadata(true, false))
            .expect_err("inactive symbols should be rejected");

        assert_eq!(error.code(), "alpaca_import_unavailable");
        assert_eq!(error.message(), "symbol 'AAPL' is inactive");
    }

    #[test]
    fn alpaca_import_rejects_active_non_tradable_symbols() {
        let error = imported_from_alpaca(alpaca_metadata(false, true))
            .expect_err("non-tradable symbols should be rejected");

        assert_eq!(error.code(), "alpaca_import_unavailable");
        assert_eq!(error.message(), "symbol 'AAPL' is not tradable");
    }

    #[test]
    fn ibkr_import_maps_contract_metadata_into_upsert_shape() {
        let imported = imported_from_ibkr(ibkr_metadata(TradingVehicleCategory::Stock));

        assert_eq!(imported.upsert.symbol, "AAPL");
        assert_eq!(imported.upsert.broker, "ibkr");
        assert_eq!(imported.upsert.category, TradingVehicleCategory::Stock);
        assert_eq!(imported.upsert.broker_asset_id.as_deref(), Some("265598"));
        assert_eq!(imported.upsert.exchange.as_deref(), Some("SMART"));
        assert_eq!(imported.upsert.broker_asset_class.as_deref(), Some("stk"));
        assert_eq!(
            imported.summary,
            "Imported from IBKR: symbol=AAPL, category=stock, sec_type=STK, conid=265598, exchange=SMART"
        );
    }

    #[test]
    fn import_from_broker_with_fetchers_uses_injected_alpaca_success_path() {
        let account = Account::default();

        let imported = import_from_broker_with_fetchers(
            &account,
            "AAPL",
            BrokerKind::Alpaca,
            None,
            alpaca_success_fetcher,
            ibkr_success_fetcher,
        )
        .expect("injected Alpaca metadata should import");

        assert_eq!(imported.upsert.broker, "alpaca");
        assert_eq!(imported.upsert.symbol, "AAPL");
    }

    #[test]
    fn import_from_broker_with_fetchers_uses_injected_ibkr_success_path() {
        let account = Account::default();

        let imported = import_from_broker_with_fetchers(
            &account,
            "AAPL",
            BrokerKind::Ibkr,
            Some(TradingVehicleCategory::Bond),
            alpaca_success_fetcher,
            ibkr_success_fetcher,
        )
        .expect("injected IBKR metadata should import");

        assert_eq!(imported.upsert.broker, "ibkr");
        assert_eq!(imported.upsert.category, TradingVehicleCategory::Bond);
    }

    #[test]
    fn import_from_broker_with_fetchers_defaults_ibkr_category_to_stock() {
        let account = Account::default();

        let imported = import_from_broker_with_fetchers(
            &account,
            "AAPL",
            BrokerKind::Ibkr,
            None,
            alpaca_success_fetcher,
            ibkr_success_fetcher,
        )
        .expect("default IBKR category should import");

        assert_eq!(imported.upsert.category, TradingVehicleCategory::Stock);
    }

    #[test]
    fn import_from_broker_wraps_pre_network_symbol_validation_errors() {
        let account = Account::default();

        let alpaca_error = import_from_broker(&account, " \t\n", BrokerKind::Alpaca, None)
            .expect_err("blank Alpaca symbol should fail before credentials");
        assert_eq!(alpaca_error.code(), "alpaca_import_failed");
        assert_eq!(alpaca_error.message(), "Symbol cannot be empty");

        let ibkr_error = import_from_broker(
            &account,
            " ",
            BrokerKind::Ibkr,
            Some(TradingVehicleCategory::Bond),
        )
        .expect_err("blank IBKR symbol should fail before client setup");
        assert_eq!(ibkr_error.code(), "ibkr_import_failed");
        assert_eq!(ibkr_error.message(), "Symbol cannot be empty");
    }

    #[test]
    fn ibkr_import_uses_description_as_exchange_fallback() {
        let imported = imported_from_ibkr(ContractMetadata {
            conid: "998877".to_string(),
            symbol: "ACME".to_string(),
            category: TradingVehicleCategory::Stock,
            sec_type: "STK".to_string(),
            company_name: Some("Acme Corp".to_string()),
            description: Some("NASDAQ Global Select".to_string()),
            currency: Some("USD".to_string()),
            exchange: None,
        });

        assert_eq!(
            imported.upsert.exchange.as_deref(),
            Some("NASDAQ Global Select")
        );
        assert_eq!(
            imported.summary,
            "Imported from IBKR: symbol=ACME, category=stock, sec_type=STK, conid=998877, exchange=-"
        );
    }

    #[test]
    fn ibkr_import_preserves_bond_category() {
        let imported = imported_from_ibkr(ContractMetadata {
            conid: "123456".to_string(),
            symbol: "9128285M8".to_string(),
            category: TradingVehicleCategory::Bond,
            sec_type: "BOND".to_string(),
            company_name: None,
            description: Some("US Treasury".to_string()),
            currency: Some("USD".to_string()),
            exchange: Some("SMART".to_string()),
        });

        assert_eq!(imported.upsert.category, TradingVehicleCategory::Bond);
        assert_eq!(imported.upsert.broker_asset_class.as_deref(), Some("bond"));
    }
}
