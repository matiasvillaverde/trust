//! Legacy instruments command responsibility. Business mutations go through TrustFacade.
use crate::command_routing::parse_trading_vehicle_subcommand;
use crate::command_routing::TradingVehicleSubcommand;
use crate::dialogs::TradingVehicleDialogBuilder;
use crate::dispatcher::accounts;
use crate::dispatcher::args;
use crate::dispatcher::context;
use crate::dispatcher::error::CliError;
use crate::dispatcher::instrument_search;
use crate::dispatcher::instrument_stats;
use crate::dispatcher::output::ReportOutputFormat;
use crate::trading_vehicle_import;
use chrono::NaiveDate;
use clap::ArgMatches;
use core::TrustFacade;
use model::BrokerKind;
use model::FixedIncomeTerms;

pub(in crate::dispatcher) fn dispatch_trading_vehicle(
    trust: &mut TrustFacade,
    sub_matches: &ArgMatches,
) -> Result<(), CliError> {
    match parse_trading_vehicle_subcommand(sub_matches) {
        TradingVehicleSubcommand::Create(sub_sub_matches) => {
            create_trading_vehicle(trust, sub_sub_matches)?
        }
        TradingVehicleSubcommand::UpdateBondTerms(sub_sub_matches) => {
            update_bond_terms(trust, sub_sub_matches)?
        }
        TradingVehicleSubcommand::Search(sub_sub_matches) => {
            instrument_search::search_trading_vehicle(trust, sub_sub_matches)?
        }
        TradingVehicleSubcommand::Stats(sub_sub_matches) => {
            instrument_stats::trading_vehicle_stats(trust, sub_sub_matches)?
        }
    }
    Ok(())
}

pub(in crate::dispatcher) fn create_trading_vehicle(
    trust: &mut TrustFacade,
    matches: &ArgMatches,
) -> Result<(), CliError> {
    context::ensure_protected_keyword(
        trust,
        matches,
        ReportOutputFormat::Text,
        "trading-vehicle creation",
    )?;
    if matches.get_flag("from-alpaca") {
        return create_trading_vehicle_from_broker(trust, matches, BrokerKind::Alpaca);
    }
    if let Some(broker) = matches.get_one::<String>("from-broker") {
        return create_trading_vehicle_from_broker(
            trust,
            matches,
            accounts::parse_broker_kind(broker)?,
        );
    }

    TradingVehicleDialogBuilder::new()
        .category()
        .symbol()
        .broker()
        .isin()
        .build(trust)
        .display();
    Ok(())
}

#[allow(clippy::too_many_lines)]
pub(in crate::dispatcher) fn create_trading_vehicle_from_broker(
    trust: &mut TrustFacade,
    matches: &ArgMatches,
    broker_kind: BrokerKind,
) -> Result<(), CliError> {
    let account_name = match matches.get_one::<String>("account") {
        Some(value) if !value.trim().is_empty() => value.trim(),
        _ => {
            return Err(CliError::new(
                "broker_import_invalid_args",
                "--account is required when importing from broker metadata",
            ));
        }
    };

    let symbol = match matches.get_one::<String>("symbol") {
        Some(value) if !value.trim().is_empty() => value.trim(),
        _ => {
            return Err(CliError::new(
                "broker_import_invalid_args",
                "--symbol is required when importing from broker metadata",
            ));
        }
    };

    let account = match trust.search_account(account_name) {
        Ok(account) => account,
        Err(error) => {
            return Err(CliError::new(
                "broker_import_account_not_found",
                format!("{error}"),
            ));
        }
    };

    let category_hint = matches
        .get_one::<String>("category")
        .map(|value| accounts::parse_trading_vehicle_category(value))
        .transpose()?;

    let imported =
        trading_vehicle_import::import_from_broker(&account, symbol, broker_kind, category_hint)
            .map_err(|error| CliError::new(error.code(), error.message().to_string()))?;

    store_imported_trading_vehicle(trust, matches, imported)
}

pub(in crate::dispatcher) fn store_imported_trading_vehicle(
    trust: &mut TrustFacade,
    matches: &ArgMatches,
    mut imported: trading_vehicle_import::ImportedTradingVehicle,
) -> Result<(), CliError> {
    imported.upsert.fixed_income = parse_fixed_income_terms(matches, imported.upsert.category)?;

    let result = trust.upsert_trading_vehicle(imported.upsert);

    match result {
        Ok(tv) => {
            println!("{}", imported.summary);
            crate::views::TradingVehicleView::display(tv);
        }
        Err(error) => {
            return Err(CliError::new(
                "broker_import_store_failed",
                format!("{error}"),
            ));
        }
    }
    Ok(())
}

pub(in crate::dispatcher) fn parse_fixed_income_terms(
    matches: &ArgMatches,
    category: model::TradingVehicleCategory,
) -> Result<Option<FixedIncomeTerms>, CliError> {
    if !fixed_income_args_present(matches) {
        return Ok(None);
    }

    if category != model::TradingVehicleCategory::Bond {
        return Err(CliError::new(
            "invalid_fixed_income_terms",
            "Bond term flags require --category bond",
        ));
    }

    Ok(Some(merge_fixed_income_terms(matches, None)?))
}

pub(in crate::dispatcher) fn update_bond_terms(
    trust: &mut TrustFacade,
    matches: &ArgMatches,
) -> Result<(), CliError> {
    context::ensure_protected_keyword(
        trust,
        matches,
        ReportOutputFormat::Text,
        "bond term update",
    )?;

    if !fixed_income_args_present(matches) {
        return Err(CliError::new(
            "missing_bond_terms",
            "At least one bond term flag is required",
        ));
    }

    let symbol = required_trimmed_arg(matches, "symbol")?;
    let broker = required_trimmed_arg(matches, "broker")?;
    let vehicle = find_trading_vehicle_by_symbol_broker(trust, symbol, broker)?;
    if vehicle.category != model::TradingVehicleCategory::Bond {
        return Err(CliError::new(
            "bond_vehicle_wrong_category",
            format!(
                "Trading vehicle {broker}:{symbol} is category {}, expected bond",
                vehicle.category
            ),
        ));
    }

    let fixed_income = merge_fixed_income_terms(matches, vehicle.fixed_income.as_ref())?;
    let updated = trust
        .upsert_trading_vehicle(upsert_from_existing_vehicle(&vehicle, Some(fixed_income)))
        .map_err(|error| CliError::new("bond_terms_update_failed", format!("{error}")))?;

    crate::views::TradingVehicleView::display(updated);
    Ok(())
}

pub(in crate::dispatcher) fn merge_fixed_income_terms(
    matches: &ArgMatches,
    existing: Option<&FixedIncomeTerms>,
) -> Result<FixedIncomeTerms, CliError> {
    Ok(FixedIncomeTerms {
        face_value: args::parse_optional_decimal_arg(
            matches,
            "face-value",
            ReportOutputFormat::Text,
        )?
        .or_else(|| existing.and_then(|terms| terms.face_value)),
        annual_coupon_rate_pct: args::parse_optional_decimal_arg(
            matches,
            "coupon-rate",
            ReportOutputFormat::Text,
        )?
        .or_else(|| existing.and_then(|terms| terms.annual_coupon_rate_pct)),
        maturity_date: parse_maturity_date_arg(matches)?
            .or_else(|| existing.and_then(|terms| terms.maturity_date)),
        coupon_frequency_per_year: parse_coupon_frequency_arg(matches)?
            .or_else(|| existing.and_then(|terms| terms.coupon_frequency_per_year)),
    })
}

pub(in crate::dispatcher) fn fixed_income_args_present(matches: &ArgMatches) -> bool {
    matches.get_one::<String>("face-value").is_some()
        || matches.get_one::<String>("coupon-rate").is_some()
        || matches.get_one::<String>("maturity-date").is_some()
        || matches.get_one::<u16>("coupon-frequency").is_some()
}

pub(in crate::dispatcher) fn parse_maturity_date_arg(
    matches: &ArgMatches,
) -> Result<Option<NaiveDate>, CliError> {
    matches
        .get_one::<String>("maturity-date")
        .map(|raw| {
            NaiveDate::parse_from_str(raw, "%Y-%m-%d").map_err(|_| {
                CliError::new(
                    "invalid_maturity_date",
                    format!("Invalid --maturity-date value (expected YYYY-MM-DD): {raw}"),
                )
            })
        })
        .transpose()
}

pub(in crate::dispatcher) fn parse_coupon_frequency_arg(
    matches: &ArgMatches,
) -> Result<Option<u16>, CliError> {
    let coupon_frequency_per_year = matches.get_one::<u16>("coupon-frequency").copied();
    if coupon_frequency_per_year == Some(0) {
        return Err(CliError::new(
            "invalid_coupon_frequency",
            "--coupon-frequency must be greater than zero",
        ));
    }
    Ok(coupon_frequency_per_year)
}

pub(in crate::dispatcher) fn required_trimmed_arg<'a>(
    matches: &'a ArgMatches,
    key: &'static str,
) -> Result<&'a str, CliError> {
    matches
        .get_one::<String>(key)
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| CliError::new("missing_argument", format!("Missing argument --{key}")))
}

pub(in crate::dispatcher) fn find_trading_vehicle_by_symbol_broker(
    trust: &mut TrustFacade,
    symbol: &str,
    broker: &str,
) -> Result<model::TradingVehicle, CliError> {
    let symbol_norm = symbol.to_uppercase();
    let broker_norm = broker.to_lowercase();
    let vehicles = trust
        .search_trading_vehicles()
        .map_err(|error| CliError::new("trading_vehicle_lookup_failed", format!("{error}")))?;
    vehicles
        .into_iter()
        .find(|vehicle| {
            vehicle.symbol == symbol_norm && vehicle.broker.eq_ignore_ascii_case(&broker_norm)
        })
        .ok_or_else(|| {
            CliError::new(
                "trading_vehicle_not_found",
                format!("No trading vehicle found for {broker}:{symbol}"),
            )
        })
}

pub(in crate::dispatcher) fn upsert_from_existing_vehicle(
    vehicle: &model::TradingVehicle,
    fixed_income: Option<FixedIncomeTerms>,
) -> model::database::TradingVehicleUpsert {
    model::database::TradingVehicleUpsert {
        symbol: vehicle.symbol.clone(),
        isin: vehicle.isin.clone(),
        category: vehicle.category,
        broker: vehicle.broker.clone(),
        broker_asset_id: vehicle.broker_asset_id.clone(),
        exchange: vehicle.exchange.clone(),
        broker_asset_class: vehicle.broker_asset_class.clone(),
        broker_asset_status: vehicle.broker_asset_status.clone(),
        tradable: vehicle.tradable,
        marginable: vehicle.marginable,
        shortable: vehicle.shortable,
        easy_to_borrow: vehicle.easy_to_borrow,
        fractionable: vehicle.fractionable,
        fixed_income,
    }
}

/// GoF Command: this group owns its workflow and uses the facade as receiver.
pub(super) struct TradingVehicleCommand;
impl super::Command for TradingVehicleCommand {
    fn execute(&self, trust: &mut TrustFacade, args: &ArgMatches) -> Result<(), CliError> {
        dispatch_trading_vehicle(trust, args)
    }
}
