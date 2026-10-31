//! Legacy instrument stats command responsibility. Business mutations go through TrustFacade.
use crate::dispatcher::error::CliError;
use crate::dispatcher::instrument_search;
use crate::dispatcher::output;
use crate::dispatcher::output::ReportOutputFormat;
use chrono::NaiveDate;
use clap::ArgMatches;
use core::TrustFacade;
use rust_decimal::Decimal;
use serde_json::json;
use serde_json::Value;
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::dispatcher) struct BondInventoryStats {
    pub(in crate::dispatcher) total: usize,
    pub(in crate::dispatcher) complete_terms: usize,
    pub(in crate::dispatcher) missing_terms: usize,
    pub(in crate::dispatcher) coupon_rate_count: usize,
    pub(in crate::dispatcher) average_coupon_rate_pct: Option<Decimal>,
    pub(in crate::dispatcher) earliest_maturity: Option<NaiveDate>,
    pub(in crate::dispatcher) latest_maturity: Option<NaiveDate>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::dispatcher) struct TradingVehicleInventoryStats {
    pub(in crate::dispatcher) total: usize,
    pub(in crate::dispatcher) by_category: BTreeMap<String, usize>,
    pub(in crate::dispatcher) by_broker: BTreeMap<String, usize>,
    pub(in crate::dispatcher) bonds: BondInventoryStats,
}

pub(in crate::dispatcher) fn trading_vehicle_stats(
    trust: &mut TrustFacade,
    matches: &ArgMatches,
) -> Result<(), CliError> {
    let format = output::parse_report_format(matches);
    let vehicles = trust
        .search_trading_vehicles()
        .map_err(|error| CliError::new("trading_vehicle_stats_failed", format!("{error}")))?;
    let stats = calculate_trading_vehicle_inventory_stats(&vehicles)?;

    match format {
        ReportOutputFormat::Text => {
            print_trading_vehicle_stats(&stats);
            Ok(())
        }
        ReportOutputFormat::Json => output::print_json(&trading_vehicle_stats_payload(&stats)),
    }
}

pub(in crate::dispatcher) fn calculate_trading_vehicle_inventory_stats(
    vehicles: &[model::TradingVehicle],
) -> Result<TradingVehicleInventoryStats, CliError> {
    let mut stats = TradingVehicleInventoryStats {
        total: vehicles.len(),
        by_category: BTreeMap::new(),
        by_broker: BTreeMap::new(),
        bonds: BondInventoryStats {
            total: 0,
            complete_terms: 0,
            missing_terms: 0,
            coupon_rate_count: 0,
            average_coupon_rate_pct: None,
            earliest_maturity: None,
            latest_maturity: None,
        },
    };
    let mut coupon_rate_sum = Decimal::ZERO;
    let mut coupon_rate_divisor = Decimal::ZERO;

    for vehicle in vehicles {
        increment_count(&mut stats.by_category, vehicle.category.to_string())?;
        increment_count(&mut stats.by_broker, vehicle.broker.to_ascii_lowercase())?;
        if vehicle.category == model::TradingVehicleCategory::Bond {
            add_bond_inventory_stats(
                &mut stats.bonds,
                vehicle,
                &mut coupon_rate_sum,
                &mut coupon_rate_divisor,
            )?;
        }
    }

    if coupon_rate_divisor > Decimal::ZERO {
        stats.bonds.average_coupon_rate_pct = Some(
            coupon_rate_sum
                .checked_div(coupon_rate_divisor)
                .ok_or_else(|| {
                    CliError::new("trading_vehicle_stats_failed", "average coupon overflow")
                })?,
        );
    }

    Ok(stats)
}

pub(in crate::dispatcher) fn add_bond_inventory_stats(
    stats: &mut BondInventoryStats,
    vehicle: &model::TradingVehicle,
    coupon_rate_sum: &mut Decimal,
    coupon_rate_divisor: &mut Decimal,
) -> Result<(), CliError> {
    stats.total = checked_increment_usize(stats.total)?;
    if instrument_search::is_bond_missing_terms(vehicle) {
        stats.missing_terms = checked_increment_usize(stats.missing_terms)?;
    } else {
        stats.complete_terms = checked_increment_usize(stats.complete_terms)?;
    }

    if let Some(terms) = &vehicle.fixed_income {
        if let Some(coupon_rate) = terms.annual_coupon_rate_pct {
            *coupon_rate_sum = coupon_rate_sum.checked_add(coupon_rate).ok_or_else(|| {
                CliError::new("trading_vehicle_stats_failed", "coupon sum overflow")
            })?;
            *coupon_rate_divisor =
                coupon_rate_divisor
                    .checked_add(Decimal::ONE)
                    .ok_or_else(|| {
                        CliError::new("trading_vehicle_stats_failed", "coupon count overflow")
                    })?;
            stats.coupon_rate_count = checked_increment_usize(stats.coupon_rate_count)?;
        }
        if let Some(maturity) = terms.maturity_date {
            stats.earliest_maturity = Some(
                stats
                    .earliest_maturity
                    .map(|current| current.min(maturity))
                    .unwrap_or(maturity),
            );
            stats.latest_maturity = Some(
                stats
                    .latest_maturity
                    .map(|current| current.max(maturity))
                    .unwrap_or(maturity),
            );
        }
    }

    Ok(())
}

pub(in crate::dispatcher) fn checked_increment_usize(value: usize) -> Result<usize, CliError> {
    value
        .checked_add(1)
        .ok_or_else(|| CliError::new("trading_vehicle_stats_failed", "count overflow"))
}

pub(in crate::dispatcher) fn increment_count(
    map: &mut BTreeMap<String, usize>,
    key: String,
) -> Result<(), CliError> {
    let next = map
        .get(&key)
        .copied()
        .unwrap_or_default()
        .checked_add(1)
        .ok_or_else(|| CliError::new("trading_vehicle_stats_failed", "count overflow"))?;
    map.insert(key, next);
    Ok(())
}

pub(in crate::dispatcher) fn print_trading_vehicle_stats(stats: &TradingVehicleInventoryStats) {
    println!("Trading Vehicle Stats");
    println!("=====================");
    println!("total: {}", stats.total);
    println!("by_category:");
    for (category, count) in &stats.by_category {
        println!("  {category}: {count}");
    }
    println!("by_broker:");
    for (broker, count) in &stats.by_broker {
        println!("  {broker}: {count}");
    }
    println!("bonds:");
    println!("  total: {}", stats.bonds.total);
    println!("  complete_terms: {}", stats.bonds.complete_terms);
    println!("  missing_terms: {}", stats.bonds.missing_terms);
    println!(
        "  average_coupon_rate: {}",
        stats
            .bonds
            .average_coupon_rate_pct
            .map(|value| format!("{}%", output::decimal_string(value)))
            .unwrap_or_else(|| "n/a".to_string())
    );
    println!(
        "  maturity_range: {}",
        bond_maturity_range_text(&stats.bonds)
    );
}

pub(in crate::dispatcher) fn bond_maturity_range_text(stats: &BondInventoryStats) -> String {
    match (stats.earliest_maturity, stats.latest_maturity) {
        (Some(earliest), Some(latest)) => format!("{earliest}..{latest}"),
        _ => "n/a".to_string(),
    }
}

pub(in crate::dispatcher) fn trading_vehicle_stats_payload(
    stats: &TradingVehicleInventoryStats,
) -> Value {
    json!({
        "report": "trading_vehicle_stats",
        "total": stats.total,
        "by_category": &stats.by_category,
        "by_broker": &stats.by_broker,
        "bonds": {
            "total": stats.bonds.total,
            "complete_terms": stats.bonds.complete_terms,
            "missing_terms": stats.bonds.missing_terms,
            "coupon_rate_count": stats.bonds.coupon_rate_count,
            "average_coupon_rate_pct": stats.bonds.average_coupon_rate_pct.map(output::decimal_string),
            "earliest_maturity": stats.bonds.earliest_maturity.map(|value| value.to_string()),
            "latest_maturity": stats.bonds.latest_maturity.map(|value| value.to_string()),
        }
    })
}
