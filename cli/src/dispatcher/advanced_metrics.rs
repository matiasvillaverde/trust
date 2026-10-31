//! Legacy advanced metrics command responsibility. Business mutations go through TrustFacade.
use crate::command_routing::parse_metrics_subcommand;
use crate::command_routing::MetricsSubcommand;
use crate::dispatcher::bond_metrics;
use crate::dispatcher::error::CliError;
use crate::dispatcher::output;
use crate::dispatcher::report_calculations;
use chrono::Utc;
use clap::ArgMatches;
use core::TrustFacade;
use model::Trade;
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use serde_json::Value;
use uuid::Uuid;

pub(in crate::dispatcher) fn dispatch_metrics(
    trust: &mut TrustFacade,
    sub_matches: &ArgMatches,
) -> Result<(), CliError> {
    match parse_metrics_subcommand(sub_matches) {
        MetricsSubcommand::Advanced(sub_sub_matches) => metrics_advanced(trust, sub_sub_matches),
        MetricsSubcommand::Compare(sub_sub_matches) => metrics_compare(trust, sub_sub_matches),
        MetricsSubcommand::Bond(sub_sub_matches) => {
            bond_metrics::metrics_bond(trust, sub_sub_matches)?
        }
    }
    Ok(())
}

pub(in crate::dispatcher) fn metrics_advanced(trust: &mut TrustFacade, sub_matches: &ArgMatches) {
    use crate::views::{AdvancedMetricsDisplayContext, AdvancedMetricsView};
    use core::calculators_performance::PerformanceCalculator;
    use rust_decimal::Decimal;
    use rust_decimal_macros::dec;
    use std::str::FromStr;
    use uuid::Uuid;

    // Get account ID if provided
    let account_id = if let Some(account_arg) = sub_matches.get_one::<String>("account") {
        match Uuid::from_str(account_arg) {
            Ok(id) => Some(id),
            Err(_) => {
                eprintln!("Error: Invalid account ID format");
                return;
            }
        }
    } else {
        None
    };

    // Get days filter if provided (default to 90)
    let days_filter = sub_matches.get_one::<u32>("days").copied().unwrap_or(90);

    // Get risk-free rate if provided (default to 5%)
    let _risk_free_rate = if let Some(rate) = sub_matches.get_one::<f64>("risk-free-rate") {
        Decimal::try_from(*rate).unwrap_or(dec!(0.05))
    } else {
        dec!(0.05)
    };

    // Display period information
    if days_filter > 0 {
        println!("Advanced Trading Metrics (Last {days_filter} days)");
    } else {
        println!("Advanced Trading Metrics (All time)");
    }
    println!("======================================");

    // Get all closed trades
    let mut all_trades = match trust.search_closed_trades(account_id) {
        Ok(trades) => trades,
        Err(_) => {
            eprintln!("Unable to retrieve trading data. Please check your account settings and try again.");
            return;
        }
    };

    // Apply days filter if specified
    if days_filter > 0 {
        all_trades = PerformanceCalculator::filter_trades_by_days(&all_trades, days_filter);
    }

    if all_trades.is_empty() {
        println!("No trades found for the specified criteria.");
        return;
    }

    // Handle export if requested
    if let Some(export_format) = sub_matches.get_one::<String>("export") {
        let output_file = sub_matches
            .get_one::<String>("output")
            .cloned()
            .unwrap_or_else(|| format!("metrics.{export_format}"));

        match export_metrics(
            trust,
            &all_trades,
            export_format,
            &output_file,
            _risk_free_rate,
        ) {
            Ok(()) => {
                println!("Metrics exported to: {output_file}");
                return;
            }
            Err(e) => {
                eprintln!("Export failed: {e}");
                // Continue with normal display
            }
        }
    }

    AdvancedMetricsView::display(
        all_trades,
        Some(AdvancedMetricsDisplayContext {
            calmar_ratio: None,
            risk_free_rate: Some(_risk_free_rate),
        }),
    );
}

pub(in crate::dispatcher) fn export_metrics(
    _trust: &mut TrustFacade,
    trades: &[Trade],
    format: &str,
    output_file: &str,
    risk_free_rate: Decimal,
) -> Result<(), Box<dyn std::error::Error>> {
    use crate::exporters::MetricsExporter;
    use std::fs::File;
    use std::io::Write;

    let content = match format {
        "json" => {
            let json = MetricsExporter::to_json(trades, Some(risk_free_rate));
            serde_json::to_string_pretty(&json)?
        }
        "csv" => MetricsExporter::to_csv(trades, Some(risk_free_rate)),
        _ => return Err("Unsupported export format".into()),
    };

    let mut file = File::create(output_file)?;
    file.write_all(content.as_bytes())?;

    Ok(())
}

#[allow(clippy::too_many_lines, clippy::indexing_slicing)]
pub(in crate::dispatcher) fn metrics_compare(trust: &mut TrustFacade, sub_matches: &ArgMatches) {
    use crate::exporters::MetricsExporter;
    use core::calculators_advanced_metrics::AdvancedMetricsCalculator;
    use serde_json::json;
    use std::fs::File;
    use std::io::Write;

    let (Ok(period1), Ok(period2)) = (
        crate::dispatcher::args::required_string_arg(sub_matches, "period1"),
        crate::dispatcher::args::required_string_arg(sub_matches, "period2"),
    ) else {
        eprintln!("Error: Provide --period1 and --period2.");
        return;
    };
    let output_format = sub_matches
        .get_one::<String>("format")
        .map(String::as_str)
        .unwrap_or("text");

    let account_id = match sub_matches.get_one::<String>("account") {
        Some(id) => match Uuid::parse_str(id) {
            Ok(v) => Some(v),
            Err(_) => {
                eprintln!("Error: Invalid account ID format");
                return;
            }
        },
        None => None,
    };

    let now = Utc::now();
    let window1 = match report_calculations::parse_period_window(period1, now) {
        Ok(v) => v,
        Err(err) => {
            eprintln!("Error: {err}");
            return;
        }
    };
    let window2 = match report_calculations::parse_period_window(period2, now) {
        Ok(v) => v,
        Err(err) => {
            eprintln!("Error: {err}");
            return;
        }
    };

    let all_trades = match trust.search_closed_trades(account_id) {
        Ok(v) => v,
        Err(err) => {
            eprintln!("Error loading trades: {err}");
            return;
        }
    };

    let p1: Vec<Trade> = all_trades
        .iter()
        .filter(|t| report_calculations::trade_in_window(t, window1.0, window1.1))
        .cloned()
        .collect();
    let p2: Vec<Trade> = all_trades
        .iter()
        .filter(|t| report_calculations::trade_in_window(t, window2.0, window2.1))
        .cloned()
        .collect();

    let make_metrics = |trades: &[Trade]| {
        let total_return = AdvancedMetricsCalculator::calculate_net_profit(trades);
        let sharpe = AdvancedMetricsCalculator::calculate_sharpe_ratio(trades, dec!(0.05));
        let profit_factor = AdvancedMetricsCalculator::calculate_profit_factor(trades);
        let win_rate = AdvancedMetricsCalculator::calculate_win_rate(trades);
        let expectancy = AdvancedMetricsCalculator::calculate_expectancy(trades);
        let max_dd = report_calculations::max_drawdown_amount(trades);
        json!({
            "trade_count": trades.len(),
            "total_return": output::decimal_string(total_return),
            "sharpe_ratio": sharpe.map(output::decimal_string),
            "profit_factor": profit_factor.map(output::decimal_string),
            "win_rate": output::decimal_string(win_rate),
            "expectancy": output::decimal_string(expectancy),
            "max_drawdown": output::decimal_string(max_dd),
        })
    };

    let p1m = make_metrics(&p1);
    let p2m = make_metrics(&p2);

    let to_decimal = |value: &Value| -> Decimal {
        value
            .as_str()
            .and_then(|s| Decimal::from_str_exact(s).ok())
            .unwrap_or(Decimal::ZERO)
    };
    let delta = |k: &str| -> String {
        let v1 = to_decimal(&p1m[k]);
        let v2 = to_decimal(&p2m[k]);
        output::decimal_string(v1.checked_sub(v2).unwrap_or(Decimal::ZERO))
    };

    let payload = json!({
        "report": "metrics_compare",
        "period1": period1,
        "period2": period2,
        "window1": {
            "from": window1.0.map(|d| d.to_string()),
            "to": window1.1.map(|d| d.to_string()),
        },
        "window2": {
            "from": window2.0.map(|d| d.to_string()),
            "to": window2.1.map(|d| d.to_string()),
        },
        "period1_metrics": p1m,
        "period2_metrics": p2m,
        "delta": {
            "total_return": delta("total_return"),
            "win_rate": delta("win_rate"),
            "expectancy": delta("expectancy"),
            "max_drawdown": delta("max_drawdown"),
        }
    });

    if let Some(export_format) = sub_matches.get_one::<String>("export") {
        let output_file = sub_matches
            .get_one::<String>("output")
            .cloned()
            .unwrap_or_else(|| format!("metrics-compare.{export_format}"));
        let content = match export_format.as_str() {
            "json" => serde_json::to_string_pretty(&payload).unwrap_or_else(|_| "{}".to_string()),
            "csv" => {
                let mut rows = vec![
                    "metric,period1,period2,delta".to_string(),
                    format!("trade_count,{},,", p1m["trade_count"].as_u64().unwrap_or(0)),
                ];
                for key in ["total_return", "win_rate", "expectancy", "max_drawdown"] {
                    rows.push(format!(
                        "{},{},{},{}",
                        key,
                        p1m[key].as_str().unwrap_or(""),
                        p2m[key].as_str().unwrap_or(""),
                        payload["delta"][key].as_str().unwrap_or("")
                    ));
                }
                rows.join("\n")
            }
            _ => MetricsExporter::to_csv(&[], None),
        };
        if let Ok(mut file) = File::create(&output_file) {
            if file.write_all(content.as_bytes()).is_ok() {
                println!("Metrics comparison exported to: {output_file}");
            } else {
                eprintln!("Failed to write export file: {output_file}");
            }
        } else {
            eprintln!("Failed to create export file: {output_file}");
        }
    }

    if output_format == "json" {
        println!("{payload}");
        return;
    }

    println!("Performance Comparison");
    println!("=====================");
    println!("Period 1 ({period1}) trades: {}", p1.len());
    println!("Period 2 ({period2}) trades: {}", p2.len());
    println!(
        "Total Return: {} vs {} (delta {})",
        p1m["total_return"].as_str().unwrap_or("0"),
        p2m["total_return"].as_str().unwrap_or("0"),
        payload["delta"]["total_return"].as_str().unwrap_or("0")
    );
    println!(
        "Win Rate: {} vs {} (delta {})",
        p1m["win_rate"].as_str().unwrap_or("0"),
        p2m["win_rate"].as_str().unwrap_or("0"),
        payload["delta"]["win_rate"].as_str().unwrap_or("0")
    );
    println!(
        "Expectancy: {} vs {} (delta {})",
        p1m["expectancy"].as_str().unwrap_or("0"),
        p2m["expectancy"].as_str().unwrap_or("0"),
        payload["delta"]["expectancy"].as_str().unwrap_or("0")
    );
    println!(
        "Max Drawdown: {} vs {} (delta {})",
        p1m["max_drawdown"].as_str().unwrap_or("0"),
        p2m["max_drawdown"].as_str().unwrap_or("0"),
        payload["delta"]["max_drawdown"].as_str().unwrap_or("0")
    );
}

/// GoF Command: this group owns its workflow and uses the facade as receiver.
pub(super) struct MetricsCommand;
impl super::Command for MetricsCommand {
    fn execute(&self, trust: &mut TrustFacade, args: &ArgMatches) -> Result<(), CliError> {
        dispatch_metrics(trust, args)
    }
}
