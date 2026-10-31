//! Legacy trade advisor command responsibility. Business mutations go through TrustFacade.
use crate::dispatcher::args;
use crate::dispatcher::context;
use crate::dispatcher::error::CliError;
use crate::dispatcher::output;
use crate::dispatcher::output::ReportOutputFormat;
use crate::dispatcher::report_calculations;
use advisor::CalendarCredentials;
use advisor::CatalystScanRequest;
use advisor::CorrelationConfig;
use advisor::CorrelationRequest;
use advisor::PositionHeat;
use advisor::RegimeConfig;
use advisor::RegimeRequest;
use chrono::Days;
use chrono::NaiveDate;
use chrono::Utc;
use clap::ArgMatches;
use core::services::AdvisoryThresholds;
use core::TrustFacade;
use model::Account;
use model::Trade;
use model::TradeEvent;
use model::TradeEventSeverity;
use model::TradeEventType;
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use serde_json::json;
use serde_json::Value;
use std::str::FromStr;

#[derive(Debug, Clone)]
pub(in crate::dispatcher) struct TradeAdvisorCheck {
    pub(in crate::dispatcher) status: &'static str,
    pub(in crate::dispatcher) payload: Value,
}

pub(in crate::dispatcher) fn trade_advisor(
    trust: &mut TrustFacade,
    sub_matches: &ArgMatches,
    format: ReportOutputFormat,
) -> Result<(), CliError> {
    let trade_id = args::parse_uuid_arg(sub_matches, "trade-id", format)?;
    let hold_days = parse_hold_days(sub_matches, format)?;
    let hold_through = sub_matches.get_flag("hold-through");
    let setup = sub_matches
        .get_one::<String>("setup")
        .map(String::as_str)
        .unwrap_or("breakout");
    let today = Utc::now().date_naive();
    let end_date = today
        .checked_add_days(Days::new(hold_days))
        .ok_or_else(|| {
            output::report_error(format, "invalid_hold_days", "--hold-days is too large")
        })?;

    let trade = trust.read_trade(trade_id).map_err(|error| {
        output::report_error(
            format,
            "trade_not_found",
            format!("Trade {trade_id} not found: {error}"),
        )
    })?;
    let account = context::account_by_id(trust, trade.account_id, format)?;
    let catalyst_scan_warning = scan_trade_catalysts(trust, &trade, today, end_date);
    let events = trust.trade_events_for_trade(trade.id).map_err(|error| {
        output::report_error(
            format,
            "trade_events_unavailable",
            format!("Failed to load trade events: {error}"),
        )
    })?;

    let catalyst = trade_advisor_catalyst_check(
        &events,
        today,
        hold_days,
        hold_through,
        catalyst_scan_warning,
    );
    let correlation = trade_advisor_correlation_check(trust, &trade, &account);
    let regime = trade_advisor_regime_check(trust, &trade, &account, setup, sub_matches);
    let verdict = trade_advisor_verdict(&[&catalyst, &correlation, &regime]);

    match format {
        ReportOutputFormat::Text => {
            println!("Trade advisor");
            println!("Trade: {} {}", trade.id, trade.trading_vehicle.symbol);
            println!("Verdict: {verdict}");
            println!("Catalyst: {}", catalyst.status);
            println!("Correlation: {}", correlation.status);
            println!("Regime: {}", regime.status);
        }
        ReportOutputFormat::Json => {
            let payload = json!({
                "trade_id": trade.id.to_string(),
                "verdict": verdict,
                "catalyst": catalyst.payload,
                "correlation": correlation.payload,
                "regime": regime.payload,
            });
            output::print_json(&payload)?;
        }
    }
    Ok(())
}

pub(in crate::dispatcher) fn trade_events(
    trust: &mut TrustFacade,
    sub_matches: &ArgMatches,
) -> Result<(), CliError> {
    match sub_matches.subcommand() {
        Some(("add", add_matches)) => trade_events_add(trust, add_matches),
        Some(("list", list_matches)) => trade_events_list(
            trust,
            list_matches,
            output::parse_report_format(list_matches),
        ),
        _ => unreachable!("No trade events subcommand provided"),
    }
}

pub(in crate::dispatcher) fn trade_events_add(
    trust: &mut TrustFacade,
    sub_matches: &ArgMatches,
) -> Result<(), CliError> {
    let format = ReportOutputFormat::Text;
    let trade_id = args::parse_uuid_arg(sub_matches, "trade-id", format)?;
    let event_type = parse_trade_event_type(sub_matches, format)?;
    let event_date = parse_date_arg(sub_matches, "date", format)?;
    let severity = parse_trade_event_severity(sub_matches, format)?;
    let notes = sub_matches.get_one::<String>("notes").cloned();

    let event = trust
        .create_trade_event(trade_id, event_type, event_date, severity, notes)
        .map_err(|error| {
            output::report_error(
                format,
                "trade_event_create_failed",
                format!("Failed to add trade event: {error}"),
            )
        })?;

    println!("Trade event added: {}", event.id);
    println!(
        "{} {} {} {}",
        event.symbol,
        trade_event_type_label(event.event_type),
        event.event_date,
        trade_event_severity_label(event.severity)
    );
    Ok(())
}

pub(in crate::dispatcher) fn trade_events_list(
    trust: &mut TrustFacade,
    sub_matches: &ArgMatches,
    format: ReportOutputFormat,
) -> Result<(), CliError> {
    let trade_id = args::parse_uuid_arg(sub_matches, "trade-id", format)?;
    let events = trust.trade_events_for_trade(trade_id).map_err(|error| {
        output::report_error(
            format,
            "trade_events_unavailable",
            format!("Failed to load trade events: {error}"),
        )
    })?;
    let today = Utc::now().date_naive();

    match format {
        ReportOutputFormat::Text => {
            println!("Trade events: {}", events.len());
            for event in &events {
                println!(
                    "{} {} {} {} {}",
                    event.id,
                    event.symbol,
                    trade_event_type_label(event.event_type),
                    event.event_date,
                    trade_event_severity_label(event.severity)
                );
            }
        }
        ReportOutputFormat::Json => {
            let payload = json!({
                "trade_id": trade_id.to_string(),
                "events": events
                    .iter()
                    .map(|event| trade_event_payload(event, today))
                    .collect::<Vec<Value>>(),
            });
            output::print_json(&payload)?;
        }
    }
    Ok(())
}

pub(in crate::dispatcher) fn scan_trade_catalysts(
    trust: &mut TrustFacade,
    trade: &Trade,
    start_date: NaiveDate,
    end_date: NaiveDate,
) -> Option<String> {
    let credentials = match CalendarCredentials::read() {
        Ok(credentials) => credentials,
        Err(error) => return Some(format!("calendar credentials unavailable: {error}")),
    };
    let request = CatalystScanRequest::new(
        trade.id,
        trade.trading_vehicle.symbol.clone(),
        start_date,
        end_date,
    );
    match trust.scan_trade_catalysts(&request, credentials) {
        Ok(result) => result.warning,
        Err(error) => Some(format!("calendar scan failed: {error}")),
    }
}

pub(in crate::dispatcher) fn trade_advisor_catalyst_check(
    events: &[TradeEvent],
    today: NaiveDate,
    hold_days: u64,
    hold_through: bool,
    scan_warning: Option<String>,
) -> TradeAdvisorCheck {
    let mut blocked = false;
    let mut warning = false;
    for event in events {
        let days_away = event.event_date.signed_duration_since(today).num_days();
        let in_hold_window =
            days_away >= 0 && u64::try_from(days_away).is_ok_and(|days| days <= hold_days);
        if !in_hold_window {
            continue;
        }
        match event.severity {
            TradeEventSeverity::High if days_away <= 3 && !hold_through => blocked = true,
            TradeEventSeverity::High | TradeEventSeverity::Medium => warning = true,
            TradeEventSeverity::Low => {}
        }
    }

    let status = if blocked {
        "BLOCKED"
    } else if warning {
        "WARNING"
    } else if scan_warning.is_some() {
        "SKIPPED"
    } else {
        "OK"
    };

    TradeAdvisorCheck {
        status,
        payload: json!({
            "events": events
                .iter()
                .map(|event| trade_event_payload(event, today))
                .collect::<Vec<Value>>(),
            "status": status,
            "warning": scan_warning,
        }),
    }
}

pub(in crate::dispatcher) fn trade_advisor_correlation_check(
    trust: &mut TrustFacade,
    trade: &Trade,
    account: &Account,
) -> TradeAdvisorCheck {
    let account_balance = match trust.search_balance(account.id, &trade.currency) {
        Ok(balance) => balance.total_balance,
        Err(error) => {
            return skipped_advisor_check(
                "correlation",
                format!("account balance unavailable: {error}"),
            );
        }
    };
    let mut open_trades = report_calculations::open_trades_for_scope(trust, Some(account.id));
    open_trades.retain(|open_trade| open_trade.id != trade.id);
    let portfolio_symbols = unique_trade_symbols(&open_trades);
    let mut heat = open_trades
        .iter()
        .map(|open_trade| {
            PositionHeat::new(
                open_trade.trading_vehicle.symbol.clone(),
                trade_risk_heat_pct(open_trade, account_balance),
            )
        })
        .collect::<Vec<PositionHeat>>();
    heat.push(PositionHeat::new(
        trade.trading_vehicle.symbol.clone(),
        trade_risk_heat_pct(trade, account_balance),
    ));

    let request = CorrelationRequest::new(trade.trading_vehicle.symbol.clone(), portfolio_symbols)
        .with_position_heat(heat);
    let advisory = match trust.correlation_advisory(&request, account, CorrelationConfig::default())
    {
        Ok(advisory) => advisory,
        Err(error) => {
            return skipped_advisor_check(
                "correlation",
                format!("correlation data unavailable: {error}"),
            );
        }
    };
    let risk_cap = trust
        .advisory_thresholds(account.id)
        .map(|thresholds| thresholds.single_position_limit_pct)
        .unwrap_or_else(|_| AdvisoryThresholds::default().single_position_limit_pct);
    let status = if advisory.heat_adjusted_pct > risk_cap {
        "BLOCKED"
    } else if advisory.max_corr > dec!(0.70) {
        "WARNING"
    } else {
        "OK"
    };

    TradeAdvisorCheck {
        status,
        payload: json!({
            "max_corr": output::decimal_string(advisory.max_corr),
            "corr_with": advisory.corr_with,
            "cluster": advisory.cluster,
            "heat_adjusted_pct": output::decimal_string(advisory.heat_adjusted_pct),
            "status": status,
        }),
    }
}

pub(in crate::dispatcher) fn trade_advisor_regime_check(
    trust: &mut TrustFacade,
    _trade: &Trade,
    account: &Account,
    setup: &str,
    sub_matches: &ArgMatches,
) -> TradeAdvisorCheck {
    let breadth_universe =
        parse_optional_symbol_list(sub_matches.get_one::<String>("breadth-universe"));
    let request = RegimeRequest::default().with_breadth_universe(breadth_universe);
    let snapshot = match trust.regime_advisory(&request, account, RegimeConfig::default()) {
        Ok(snapshot) => snapshot,
        Err(error) => {
            return skipped_advisor_check("regime", format!("regime data unavailable: {error}"));
        }
    };
    let permitted = match setup {
        "mean-reversion" => snapshot.mean_reversion_permitted,
        _ => snapshot.breakouts_permitted,
    };
    let status = if permitted { "OK" } else { "BLOCKED" };

    TradeAdvisorCheck {
        status,
        payload: json!({
            "vol": vol_regime_label(snapshot.vol_regime),
            "trend": trend_regime_label(snapshot.trend_regime),
            "breadth": snapshot.breadth_regime.map(breadth_regime_label),
            "composite": composite_regime_label(snapshot.composite),
            "breakouts_permitted": snapshot.breakouts_permitted,
            "status": status,
        }),
    }
}

pub(in crate::dispatcher) fn skipped_advisor_check(
    kind: &'static str,
    warning: String,
) -> TradeAdvisorCheck {
    let payload = match kind {
        "correlation" => json!({
            "max_corr": null,
            "corr_with": null,
            "cluster": null,
            "heat_adjusted_pct": null,
            "status": "SKIPPED",
            "warning": warning,
        }),
        "regime" => json!({
            "vol": null,
            "trend": null,
            "breadth": null,
            "composite": null,
            "breakouts_permitted": null,
            "status": "SKIPPED",
            "warning": warning,
        }),
        _ => json!({
            "status": "SKIPPED",
            "warning": warning,
        }),
    };
    TradeAdvisorCheck {
        status: "SKIPPED",
        payload,
    }
}

pub(in crate::dispatcher) fn trade_advisor_verdict(checks: &[&TradeAdvisorCheck]) -> &'static str {
    if checks.iter().any(|check| check.status == "BLOCKED") {
        return "NO_TRADE";
    }
    if checks.iter().any(|check| check.status == "WARNING") {
        return "CAUTION";
    }
    "PROCEED"
}

pub(in crate::dispatcher) fn trade_event_payload(event: &TradeEvent, today: NaiveDate) -> Value {
    json!({
        "id": event.id.to_string(),
        "trade_id": event.trade_id.to_string(),
        "symbol": event.symbol,
        "type": trade_event_type_label(event.event_type),
        "date": event.event_date.to_string(),
        "severity": trade_event_severity_label(event.severity),
        "source": event.source.to_string(),
        "notes": event.notes,
        "days_away": event.event_date.signed_duration_since(today).num_days(),
    })
}

pub(in crate::dispatcher) fn parse_hold_days(
    sub_matches: &ArgMatches,
    format: ReportOutputFormat,
) -> Result<u64, CliError> {
    let raw = sub_matches
        .get_one::<String>("hold-days")
        .map(String::as_str)
        .unwrap_or("30");
    raw.parse::<u64>().map_err(|_| {
        output::report_error(
            format,
            "invalid_hold_days",
            format!("Invalid --hold-days value: {raw}"),
        )
    })
}

pub(in crate::dispatcher) fn parse_date_arg(
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
            format!("Invalid --{key} date: {raw}. Use YYYY-MM-DD."),
        )
    })
}

pub(in crate::dispatcher) fn parse_trade_event_type(
    sub_matches: &ArgMatches,
    format: ReportOutputFormat,
) -> Result<TradeEventType, CliError> {
    let raw = sub_matches.get_one::<String>("type").ok_or_else(|| {
        output::report_error(format, "missing_argument", "Missing argument --type")
    })?;
    TradeEventType::from_str(raw).map_err(|_| {
        output::report_error(
            format,
            "invalid_trade_event_type",
            format!("Invalid --type value: {raw}"),
        )
    })
}

pub(in crate::dispatcher) fn parse_trade_event_severity(
    sub_matches: &ArgMatches,
    format: ReportOutputFormat,
) -> Result<TradeEventSeverity, CliError> {
    let raw = sub_matches.get_one::<String>("severity").ok_or_else(|| {
        output::report_error(format, "missing_argument", "Missing argument --severity")
    })?;
    TradeEventSeverity::from_str(raw).map_err(|_| {
        output::report_error(
            format,
            "invalid_trade_event_severity",
            format!("Invalid --severity value: {raw}"),
        )
    })
}

pub(in crate::dispatcher) fn unique_trade_symbols(trades: &[Trade]) -> Vec<String> {
    let mut symbols = trades
        .iter()
        .map(|trade| trade.trading_vehicle.symbol.clone())
        .collect::<Vec<String>>();
    symbols.sort();
    symbols.dedup();
    symbols
}

pub(in crate::dispatcher) fn trade_risk_heat_pct(
    trade: &Trade,
    account_balance: Decimal,
) -> Decimal {
    if account_balance <= Decimal::ZERO {
        return Decimal::ZERO;
    }
    let risk_per_share = trade
        .entry
        .unit_price
        .checked_sub(trade.safety_stop.unit_price)
        .map(|value| value.abs())
        .unwrap_or(Decimal::ZERO);
    risk_per_share
        .checked_mul(trade.entry.quantity)
        .and_then(|risk| risk.checked_mul(dec!(100)))
        .and_then(|risk_pct| risk_pct.checked_div(account_balance))
        .unwrap_or(Decimal::ZERO)
}

pub(in crate::dispatcher) fn parse_optional_symbol_list(raw: Option<&String>) -> Vec<String> {
    raw.map(|symbols| {
        symbols
            .split(',')
            .map(str::trim)
            .filter(|symbol| !symbol.is_empty())
            .map(str::to_ascii_uppercase)
            .collect::<Vec<String>>()
    })
    .unwrap_or_default()
}

pub(in crate::dispatcher) fn trade_event_type_label(event_type: TradeEventType) -> &'static str {
    match event_type {
        TradeEventType::Earnings => "Earnings",
        TradeEventType::Fed => "Fed",
        TradeEventType::Cpi => "CPI",
        TradeEventType::Nfp => "NFP",
        TradeEventType::ExDividend => "ExDividend",
        TradeEventType::Guidance => "Guidance",
        TradeEventType::Other => "Other",
    }
}

pub(in crate::dispatcher) fn trade_event_severity_label(
    severity: TradeEventSeverity,
) -> &'static str {
    match severity {
        TradeEventSeverity::Low => "Low",
        TradeEventSeverity::Medium => "Medium",
        TradeEventSeverity::High => "High",
    }
}

pub(in crate::dispatcher) fn vol_regime_label(regime: advisor::VolRegime) -> &'static str {
    match regime {
        advisor::VolRegime::Calm => "Calm",
        advisor::VolRegime::Normal => "Normal",
        advisor::VolRegime::Elevated => "Elevated",
    }
}

pub(in crate::dispatcher) fn trend_regime_label(regime: advisor::TrendRegime) -> &'static str {
    match regime {
        advisor::TrendRegime::Choppy => "Choppy",
        advisor::TrendRegime::Trending => "Trending",
        advisor::TrendRegime::StrongTrend => "StrongTrend",
    }
}

pub(in crate::dispatcher) fn breadth_regime_label(regime: advisor::BreadthRegime) -> &'static str {
    match regime {
        advisor::BreadthRegime::Broad => "Broad",
        advisor::BreadthRegime::Narrow => "Narrow",
    }
}

pub(in crate::dispatcher) fn composite_regime_label(
    regime: advisor::CompositeRegime,
) -> &'static str {
    match regime {
        advisor::CompositeRegime::CalmTrending => "Calm",
        advisor::CompositeRegime::NormalTrending => "Normal",
        advisor::CompositeRegime::ElevatedTrending => "Elevated",
        advisor::CompositeRegime::Choppy => "Choppy",
        advisor::CompositeRegime::ElevatedChoppy => "ElevatedChoppy",
        advisor::CompositeRegime::NarrowBreadth => "NarrowBreadth",
    }
}
