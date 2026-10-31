//! Legacy bond metrics command responsibility. Business mutations go through TrustFacade.
use crate::dispatcher::args;
use crate::dispatcher::error::CliError;
use crate::dispatcher::output;
use crate::dispatcher::output::ReportOutputFormat;
use chrono::NaiveDate;
use chrono::Utc;
use clap::ArgMatches;
use core::TrustFacade;
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use serde_json::json;
use serde_json::Value;
use std::str::FromStr;

pub(in crate::dispatcher) fn metrics_bond(
    trust: &mut TrustFacade,
    sub_matches: &ArgMatches,
) -> Result<(), CliError> {
    let format = output::parse_report_format(sub_matches);
    let input = parse_bond_analytics_input(trust, sub_matches, format)?;
    let analytics = trust
        .calculate_bond_analytics(input.clone())
        .map_err(|error| {
            output::report_error(
                format,
                "bond_analytics_failed",
                format!("Unable to calculate bond metrics: {error}"),
            )
        })?;

    match format {
        ReportOutputFormat::Text => {
            print_bond_metrics_text(&input, &analytics);
            Ok(())
        }
        ReportOutputFormat::Json => output::print_json(&bond_metrics_payload(&input, &analytics)),
    }
}

pub(in crate::dispatcher) fn parse_bond_analytics_input(
    trust: &mut TrustFacade,
    sub_matches: &ArgMatches,
    format: ReportOutputFormat,
) -> Result<core::calculators_fixed_income::BondAnalyticsInput, CliError> {
    let quantity = sub_matches
        .get_one::<i64>("quantity")
        .copied()
        .ok_or_else(|| {
            output::report_error(format, "missing_argument", "Missing argument --quantity")
        })?;

    let market_price = args::parse_decimal_arg(sub_matches, "market-price", format)?;
    let symbol = sub_matches
        .get_one::<String>("symbol")
        .map(|value| value.trim())
        .filter(|value| !value.is_empty());
    let broker = sub_matches
        .get_one::<String>("broker")
        .map(|value| value.trim())
        .filter(|value| !value.is_empty());

    match (symbol, broker) {
        (Some(symbol), Some(broker)) => parse_bond_input_from_vehicle(
            trust,
            sub_matches,
            symbol,
            broker,
            market_price,
            quantity,
            format,
        ),
        (Some(_), None) => Err(output::report_error(
            format,
            "missing_argument",
            "Missing argument --broker for stored bond lookup",
        )),
        (None, Some(_)) => Err(output::report_error(
            format,
            "missing_argument",
            "Missing argument --symbol for stored bond lookup",
        )),
        (None, None) => Ok(core::calculators_fixed_income::BondAnalyticsInput {
            face_value: args::parse_decimal_arg(sub_matches, "face-value", format)?,
            market_price,
            annual_coupon_rate_pct: args::parse_decimal_arg(sub_matches, "coupon-rate", format)?,
            quantity,
            years_to_maturity: args::parse_decimal_arg(sub_matches, "years-to-maturity", format)?,
            accrued_interest: parse_bond_accrued_interest_input(sub_matches, None, format)?,
        }),
    }
}

pub(in crate::dispatcher) fn parse_bond_input_from_vehicle(
    trust: &mut TrustFacade,
    sub_matches: &ArgMatches,
    symbol: &str,
    broker: &str,
    market_price: Decimal,
    quantity: i64,
    format: ReportOutputFormat,
) -> Result<core::calculators_fixed_income::BondAnalyticsInput, CliError> {
    let symbol_norm = symbol.to_uppercase();
    let broker_norm = broker.to_lowercase();
    let vehicles = trust.search_trading_vehicles().map_err(|error| {
        output::report_error(format, "bond_vehicle_lookup_failed", format!("{error}"))
    })?;
    let vehicle = vehicles
        .into_iter()
        .find(|vehicle| {
            vehicle.symbol == symbol_norm && vehicle.broker.eq_ignore_ascii_case(&broker_norm)
        })
        .ok_or_else(|| {
            output::report_error(
                format,
                "bond_vehicle_not_found",
                format!("No trading vehicle found for {broker}:{symbol}"),
            )
        })?;

    if vehicle.category != model::TradingVehicleCategory::Bond {
        return Err(output::report_error(
            format,
            "bond_vehicle_wrong_category",
            format!(
                "Trading vehicle {broker}:{symbol} is category {}, expected bond",
                vehicle.category
            ),
        ));
    }

    let terms = vehicle.fixed_income.as_ref().ok_or_else(|| {
        output::report_error(
            format,
            "bond_terms_missing",
            format!("Trading vehicle {broker}:{symbol} has no fixed-income terms"),
        )
    })?;

    Ok(core::calculators_fixed_income::BondAnalyticsInput {
        face_value: decimal_arg_or_stored(
            sub_matches,
            "face-value",
            terms.face_value,
            "stored bond face value is missing; pass --face-value",
            format,
        )?,
        market_price,
        annual_coupon_rate_pct: decimal_arg_or_stored(
            sub_matches,
            "coupon-rate",
            terms.annual_coupon_rate_pct,
            "stored bond coupon rate is missing; pass --coupon-rate",
            format,
        )?,
        quantity,
        years_to_maturity: years_to_maturity_arg_or_stored(
            sub_matches,
            terms.maturity_date,
            format,
        )?,
        accrued_interest: parse_bond_accrued_interest_input(
            sub_matches,
            terms.coupon_frequency_per_year,
            format,
        )?,
    })
}

pub(in crate::dispatcher) fn parse_bond_accrued_interest_input(
    sub_matches: &ArgMatches,
    stored_coupon_frequency: Option<u16>,
    format: ReportOutputFormat,
) -> Result<Option<core::calculators_fixed_income::BondAccruedInterestInput>, CliError> {
    if !bond_accrued_interest_args_present(sub_matches) {
        return Ok(None);
    }

    let coupon_frequency = sub_matches
        .get_one::<u16>("coupon-frequency")
        .copied()
        .or(stored_coupon_frequency)
        .ok_or_else(|| {
            output::report_error(
                format,
                "missing_argument",
                "Missing argument --coupon-frequency for accrued-interest calculations",
            )
        })?;
    if coupon_frequency == 0 {
        return Err(output::report_error(
            format,
            "invalid_coupon_frequency",
            "--coupon-frequency must be greater than zero",
        ));
    }

    let day_count_basis = match sub_matches.get_one::<String>("day-count") {
        Some(raw) => {
            core::calculators_fixed_income::DayCountBasis::from_str(raw).map_err(|_| {
                output::report_error(
                    format,
                    "invalid_day_count",
                    format!("Invalid --day-count value: {raw}"),
                )
            })?
        }
        None => core::calculators_fixed_income::DayCountBasis::ActualActual,
    };

    Ok(Some(
        core::calculators_fixed_income::BondAccruedInterestInput {
            settlement_date: parse_required_bond_date_arg(sub_matches, "settlement-date", format)?,
            last_coupon_date: parse_required_bond_date_arg(
                sub_matches,
                "last-coupon-date",
                format,
            )?,
            next_coupon_date: parse_required_bond_date_arg(
                sub_matches,
                "next-coupon-date",
                format,
            )?,
            coupon_frequency_per_year: coupon_frequency,
            day_count_basis,
        },
    ))
}

pub(in crate::dispatcher) fn bond_accrued_interest_args_present(sub_matches: &ArgMatches) -> bool {
    sub_matches.get_one::<u16>("coupon-frequency").is_some()
        || sub_matches.get_one::<String>("settlement-date").is_some()
        || sub_matches.get_one::<String>("last-coupon-date").is_some()
        || sub_matches.get_one::<String>("next-coupon-date").is_some()
        || sub_matches.get_one::<String>("day-count").is_some()
}

pub(in crate::dispatcher) fn parse_required_bond_date_arg(
    sub_matches: &ArgMatches,
    key: &'static str,
    format: ReportOutputFormat,
) -> Result<NaiveDate, CliError> {
    let raw = sub_matches.get_one::<String>(key).ok_or_else(|| {
        output::report_error(
            format,
            "missing_argument",
            format!("Missing argument --{key}"),
        )
    })?;
    NaiveDate::parse_from_str(raw, "%Y-%m-%d").map_err(|_| {
        output::report_error(
            format,
            "invalid_date",
            format!("Invalid --{key} value (expected YYYY-MM-DD): {raw}"),
        )
    })
}

pub(in crate::dispatcher) fn decimal_arg_or_stored(
    sub_matches: &ArgMatches,
    key: &'static str,
    stored: Option<Decimal>,
    missing_message: &'static str,
    format: ReportOutputFormat,
) -> Result<Decimal, CliError> {
    match args::parse_optional_decimal_arg(sub_matches, key, format)? {
        Some(value) => Ok(value),
        None => stored
            .ok_or_else(|| output::report_error(format, "bond_terms_missing", missing_message)),
    }
}

pub(in crate::dispatcher) fn years_to_maturity_arg_or_stored(
    sub_matches: &ArgMatches,
    maturity_date: Option<NaiveDate>,
    format: ReportOutputFormat,
) -> Result<Decimal, CliError> {
    if let Some(value) = args::parse_optional_decimal_arg(sub_matches, "years-to-maturity", format)?
    {
        return Ok(value);
    }

    let maturity_date = maturity_date.ok_or_else(|| {
        output::report_error(
            format,
            "bond_terms_missing",
            "stored bond maturity date is missing; pass --years-to-maturity",
        )
    })?;
    years_to_maturity(maturity_date, Utc::now().date_naive(), format)
}

pub(in crate::dispatcher) fn years_to_maturity(
    maturity_date: NaiveDate,
    as_of_date: NaiveDate,
    _format: ReportOutputFormat,
) -> Result<Decimal, CliError> {
    let days = maturity_date.signed_duration_since(as_of_date).num_days();
    if days <= 0 {
        return Ok(Decimal::ZERO);
    }
    Ok(Decimal::from(days)
        .checked_div(dec!(365))
        .unwrap_or(dec!(0)))
}

pub(in crate::dispatcher) fn print_bond_metrics_text(
    input: &core::calculators_fixed_income::BondAnalyticsInput,
    analytics: &core::calculators_fixed_income::BondAnalytics,
) {
    println!("Bond Metrics");
    println!("============");
    println!(
        "face_value: {}",
        crate::zen::amount_share(input.face_value, input.face_value)
    );
    println!(
        "market_price: {}",
        crate::zen::amount_share(input.market_price, input.face_value)
    );
    println!(
        "coupon_rate: {}%",
        output::decimal_string(input.annual_coupon_rate_pct)
    );
    println!("quantity: {}", input.quantity);
    println!(
        "position_face_value: {}",
        crate::zen::amount_share(analytics.position_face_value, analytics.position_face_value)
    );
    println!(
        "position_market_value: {}",
        crate::zen::amount_share(
            analytics.position_market_value,
            analytics.position_face_value,
        )
    );
    println!(
        "accrued_interest: {}",
        crate::zen::amount_share(
            analytics.accrued_interest_total,
            analytics.position_face_value,
        )
    );
    println!(
        "dirty_price: {}",
        crate::zen::amount_share(analytics.dirty_price, input.face_value)
    );
    println!(
        "position_dirty_value: {}",
        crate::zen::amount_share(
            analytics.position_dirty_value,
            analytics.position_face_value
        )
    );
    println!(
        "annual_coupon_income: {}",
        crate::zen::amount_share(
            analytics.annual_coupon_income,
            analytics.position_face_value
        )
    );
    println!(
        "current_yield: {}%",
        output::decimal_string(analytics.current_yield_pct)
    );
    println!(
        "approximate_ytm: {}",
        analytics
            .approximate_yield_to_maturity_pct
            .map(|value| format!("{}%", output::decimal_string(value)))
            .unwrap_or_else(|| "n/a".to_string())
    );
    println!(
        "premium_discount: {} ({}%)",
        crate::zen::amount_share(analytics.price_premium_discount, input.face_value),
        output::decimal_string(analytics.price_premium_discount_pct)
    );
}

pub(in crate::dispatcher) fn bond_metrics_payload(
    input: &core::calculators_fixed_income::BondAnalyticsInput,
    analytics: &core::calculators_fixed_income::BondAnalytics,
) -> Value {
    json!({
        "report": "bond_metrics",
        "input": {
            "face_value": output::decimal_string(input.face_value),
            "market_price": output::decimal_string(input.market_price),
            "annual_coupon_rate_pct": output::decimal_string(input.annual_coupon_rate_pct),
            "quantity": input.quantity,
            "years_to_maturity": output::decimal_string(input.years_to_maturity),
            "accrued_interest": input.accrued_interest.as_ref().map(|accrual| json!({
                "settlement_date": accrual.settlement_date.to_string(),
                "last_coupon_date": accrual.last_coupon_date.to_string(),
                "next_coupon_date": accrual.next_coupon_date.to_string(),
                "coupon_frequency_per_year": accrual.coupon_frequency_per_year,
                "day_count_basis": accrual.day_count_basis.to_string(),
            })),
        },
        "data": {
            "position_face_value": output::decimal_string(analytics.position_face_value),
            "position_market_value": output::decimal_string(analytics.position_market_value),
            "annual_coupon_per_unit": output::decimal_string(analytics.annual_coupon_per_unit),
            "annual_coupon_income": output::decimal_string(analytics.annual_coupon_income),
            "current_yield_pct": output::decimal_string(analytics.current_yield_pct),
            "approximate_yield_to_maturity_pct": analytics.approximate_yield_to_maturity_pct.map(output::decimal_string),
            "price_premium_discount": output::decimal_string(analytics.price_premium_discount),
            "price_premium_discount_pct": output::decimal_string(analytics.price_premium_discount_pct),
            "accrued_interest_per_unit": output::decimal_string(analytics.accrued_interest_per_unit),
            "accrued_interest_total": output::decimal_string(analytics.accrued_interest_total),
            "dirty_price": output::decimal_string(analytics.dirty_price),
            "position_dirty_value": output::decimal_string(analytics.position_dirty_value),
        }
    })
}
