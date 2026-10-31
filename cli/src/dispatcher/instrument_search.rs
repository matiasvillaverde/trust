//! Legacy instrument search command responsibility. Business mutations go through TrustFacade.
use crate::dialogs::TradingVehicleSearchDialogBuilder;
use crate::dispatcher::accounts;
use crate::dispatcher::error::CliError;
use crate::dispatcher::output;
use crate::dispatcher::output::ReportOutputFormat;
use clap::ArgMatches;
use core::TrustFacade;
use serde_json::json;
use serde_json::Value;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::dispatcher) struct TradingVehicleSearchFilters {
    pub(in crate::dispatcher) category: Option<model::TradingVehicleCategory>,
    pub(in crate::dispatcher) broker: Option<String>,
    pub(in crate::dispatcher) symbol: Option<String>,
    pub(in crate::dispatcher) isin: Option<String>,
    pub(in crate::dispatcher) missing_bond_terms: bool,
    pub(in crate::dispatcher) list_all: bool,
    pub(in crate::dispatcher) format: ReportOutputFormat,
}

pub(in crate::dispatcher) fn search_trading_vehicle(
    trust: &mut TrustFacade,
    matches: &ArgMatches,
) -> Result<(), CliError> {
    let filters = parse_trading_vehicle_search_filters(matches)?;
    if !filters.has_non_interactive_filters() {
        TradingVehicleSearchDialogBuilder::new()
            .search(trust)
            .display();
        return Ok(());
    }

    let vehicles = trust
        .search_trading_vehicles()
        .map_err(|error| CliError::new("trading_vehicle_search_failed", format!("{error}")))?;
    let filtered = filter_trading_vehicles(vehicles, &filters);

    match filters.format {
        ReportOutputFormat::Text => {
            if filtered.is_empty() {
                println!("No trading vehicles matched the filters.");
            } else {
                crate::views::TradingVehicleView::display_table(filtered);
            }
            Ok(())
        }
        ReportOutputFormat::Json => {
            output::print_json(&trading_vehicle_search_payload(&filtered, &filters))
        }
    }
}

pub(in crate::dispatcher) fn parse_trading_vehicle_search_filters(
    matches: &ArgMatches,
) -> Result<TradingVehicleSearchFilters, CliError> {
    Ok(TradingVehicleSearchFilters {
        category: matches
            .get_one::<String>("category")
            .map(|value| accounts::parse_trading_vehicle_category(value))
            .transpose()?,
        broker: optional_trimmed_lowercase_arg(matches, "broker"),
        symbol: optional_trimmed_uppercase_arg(matches, "symbol"),
        isin: optional_trimmed_uppercase_arg(matches, "isin"),
        missing_bond_terms: matches.get_flag("missing-bond-terms"),
        list_all: matches.get_flag("all"),
        format: output::parse_report_format(matches),
    })
}

pub(in crate::dispatcher) fn optional_trimmed_lowercase_arg(
    matches: &ArgMatches,
    key: &'static str,
) -> Option<String> {
    matches
        .get_one::<String>(key)
        .map(|value| value.trim().to_ascii_lowercase())
        .filter(|value| !value.is_empty())
}

pub(in crate::dispatcher) fn optional_trimmed_uppercase_arg(
    matches: &ArgMatches,
    key: &'static str,
) -> Option<String> {
    matches
        .get_one::<String>(key)
        .map(|value| value.trim().to_ascii_uppercase())
        .filter(|value| !value.is_empty())
}

pub(in crate::dispatcher) fn filter_trading_vehicles(
    vehicles: Vec<model::TradingVehicle>,
    filters: &TradingVehicleSearchFilters,
) -> Vec<model::TradingVehicle> {
    let mut filtered: Vec<model::TradingVehicle> = vehicles
        .into_iter()
        .filter(|vehicle| trading_vehicle_matches_filters(vehicle, filters))
        .collect();
    filtered.sort_by(|left, right| {
        left.category
            .to_string()
            .cmp(&right.category.to_string())
            .then_with(|| left.symbol.cmp(&right.symbol))
            .then_with(|| left.broker.cmp(&right.broker))
    });
    filtered
}

pub(in crate::dispatcher) fn trading_vehicle_matches_filters(
    vehicle: &model::TradingVehicle,
    filters: &TradingVehicleSearchFilters,
) -> bool {
    if filters
        .category
        .is_some_and(|category| vehicle.category != category)
    {
        return false;
    }
    if filters
        .broker
        .as_ref()
        .is_some_and(|broker| !vehicle.broker.to_ascii_lowercase().contains(broker))
    {
        return false;
    }
    if filters
        .symbol
        .as_ref()
        .is_some_and(|symbol| !vehicle.symbol.to_ascii_uppercase().contains(symbol))
    {
        return false;
    }
    if filters.isin.as_ref().is_some_and(|isin| {
        vehicle
            .isin
            .as_ref()
            .is_none_or(|value| !value.to_ascii_uppercase().contains(isin))
    }) {
        return false;
    }
    if filters.missing_bond_terms && !is_bond_missing_terms(vehicle) {
        return false;
    }
    true
}

pub(in crate::dispatcher) fn is_bond_missing_terms(vehicle: &model::TradingVehicle) -> bool {
    if vehicle.category != model::TradingVehicleCategory::Bond {
        return false;
    }
    match &vehicle.fixed_income {
        Some(terms) => {
            terms.face_value.is_none()
                || terms.annual_coupon_rate_pct.is_none()
                || terms.maturity_date.is_none()
                || terms.coupon_frequency_per_year.is_none()
        }
        None => true,
    }
}

pub(in crate::dispatcher) fn trading_vehicle_search_payload(
    vehicles: &[model::TradingVehicle],
    filters: &TradingVehicleSearchFilters,
) -> Value {
    json!({
        "report": "trading_vehicle_search",
        "filters": {
            "category": filters.category.map(|category| category.to_string()),
            "broker": &filters.broker,
            "symbol": &filters.symbol,
            "isin": &filters.isin,
            "missing_bond_terms": filters.missing_bond_terms,
            "all": filters.list_all,
        },
        "count": vehicles.len(),
        "data": vehicles.iter().map(trading_vehicle_payload).collect::<Vec<_>>(),
    })
}

pub(in crate::dispatcher) fn trading_vehicle_payload(vehicle: &model::TradingVehicle) -> Value {
    json!({
        "id": vehicle.id.to_string(),
        "category": vehicle.category.to_string(),
        "symbol": &vehicle.symbol,
        "broker": &vehicle.broker,
        "isin": &vehicle.isin,
        "broker_asset_id": &vehicle.broker_asset_id,
        "exchange": &vehicle.exchange,
        "broker_asset_class": &vehicle.broker_asset_class,
        "tradable": vehicle.tradable,
        "marginable": vehicle.marginable,
        "shortable": vehicle.shortable,
        "fractionable": vehicle.fractionable,
        "fixed_income": vehicle.fixed_income.as_ref().map(|terms| json!({
            "face_value": terms.face_value.map(output::decimal_string),
            "annual_coupon_rate_pct": terms.annual_coupon_rate_pct.map(output::decimal_string),
            "maturity_date": terms.maturity_date.map(|value| value.to_string()),
            "coupon_frequency_per_year": terms.coupon_frequency_per_year,
        })),
    })
}

#[cfg(test)]
pub(in crate::dispatcher) fn category_to_str(
    category: model::TradingVehicleCategory,
) -> &'static str {
    match category {
        model::TradingVehicleCategory::Crypto => "crypto",
        model::TradingVehicleCategory::Fiat => "fiat",
        model::TradingVehicleCategory::Stock => "stock",
        model::TradingVehicleCategory::Etf => "etf",
        model::TradingVehicleCategory::Bond => "bond",
        _ => "unknown",
    }
}

impl TradingVehicleSearchFilters {
    pub(in crate::dispatcher) fn has_non_interactive_filters(&self) -> bool {
        self.list_all
            || self.category.is_some()
            || self.broker.is_some()
            || self.symbol.is_some()
            || self.isin.is_some()
            || self.missing_bond_terms
            || self.format == ReportOutputFormat::Json
    }
}
