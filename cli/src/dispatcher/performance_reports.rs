//! Legacy performance reports command responsibility. Business mutations go through TrustFacade.
use crate::dispatcher::error::CliError;
use crate::dispatcher::output;
use crate::dispatcher::output::ReportOutputFormat;
use clap::ArgMatches;
use core::TrustFacade;
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use serde_json::json;
use uuid::Uuid;

#[allow(clippy::too_many_lines)]
pub(in crate::dispatcher) fn drawdown_report(
    trust: &mut TrustFacade,
    sub_matches: &ArgMatches,
    format: ReportOutputFormat,
) -> Result<(), CliError> {
    use crate::views::DrawdownView;
    use core::calculators_drawdown::RealizedDrawdownCalculator;
    use std::str::FromStr;

    // Get account ID if provided
    let account_id = if let Some(account_arg) = sub_matches.get_one::<String>("account") {
        match Uuid::from_str(account_arg) {
            Ok(id) => Some(id),
            Err(_) => {
                return Err(output::report_error(
                    format,
                    "invalid_account_id",
                    "Invalid account ID format",
                ));
            }
        }
    } else {
        None
    };

    // Fetch transactions for the account
    let transactions = if let Some(account_id) = account_id {
        match trust.get_account_transactions(account_id) {
            Ok(txns) => txns,
            Err(_) => {
                return Err(output::report_error(
                        format,
                        "transaction_data_unavailable",
                        "Unable to retrieve transaction data. Please check your account settings and try again.",
                    ));
            }
        }
    } else {
        // If no account specified, get all transactions for all accounts
        match trust.get_all_transactions() {
            Ok(txns) => txns,
            Err(_) => {
                return Err(output::report_error(
                        format,
                        "transaction_data_unavailable",
                        "Unable to retrieve transaction data. Please check your settings and try again.",
                    ));
            }
        }
    };

    // Calculate equity curve
    let curve = match RealizedDrawdownCalculator::calculate_equity_curve(&transactions) {
        Ok(c) => c,
        Err(e) => {
            return Err(output::report_error(
                format,
                "drawdown_equity_curve_failed",
                format!("Error calculating equity curve: {e}"),
            ));
        }
    };

    // Calculate drawdown metrics
    let metrics = match RealizedDrawdownCalculator::calculate_drawdown_metrics(&curve) {
        Ok(m) => m,
        Err(e) => {
            return Err(output::report_error(
                format,
                "drawdown_calculation_failed",
                format!("Error calculating drawdown metrics: {e}"),
            ));
        }
    };

    match format {
        ReportOutputFormat::Text => DrawdownView::display(metrics),
        ReportOutputFormat::Json => {
            let payload = json!({
                "report": "drawdown",
                "format_version": 1,
                "generated_at": chrono::Utc::now().to_rfc3339(),
                "scope": { "account_id": account_id.map(|id| id.to_string()) },
                "data": {
                    "current_equity": output::decimal_string(metrics.current_equity),
                    "peak_equity": output::decimal_string(metrics.peak_equity),
                    "current_drawdown": output::decimal_string(metrics.current_drawdown),
                    "current_drawdown_percentage": output::decimal_string(metrics.current_drawdown_percentage),
                    "max_drawdown": output::decimal_string(metrics.max_drawdown),
                    "max_drawdown_percentage": output::decimal_string(metrics.max_drawdown_percentage),
                    "max_drawdown_date": metrics.max_drawdown_date.map(|d| d.format("%Y-%m-%dT%H:%M:%S").to_string()),
                    "recovery_from_max": output::decimal_string(metrics.recovery_from_max),
                    "recovery_percentage": output::decimal_string(metrics.recovery_percentage),
                    "days_since_peak": metrics.days_since_peak,
                    "days_in_drawdown": metrics.days_in_drawdown
                },
                "consistency": {
                    "drawdown_non_negative": metrics.current_drawdown >= dec!(0) && metrics.max_drawdown >= dec!(0),
                    "max_ge_current": metrics.max_drawdown >= metrics.current_drawdown
                }
            });
            output::print_json(&payload)?;
        }
    }

    Ok(())
}

pub(in crate::dispatcher) fn performance_report(
    trust: &mut TrustFacade,
    sub_matches: &ArgMatches,
    format: ReportOutputFormat,
) -> Result<(), CliError> {
    use crate::views::PerformanceView;
    use core::calculators_performance::PerformanceCalculator;
    use std::str::FromStr;

    // Get account ID if provided
    let account_id = if let Some(account_arg) = sub_matches.get_one::<String>("account") {
        match Uuid::from_str(account_arg) {
            Ok(id) => Some(id),
            Err(_) => {
                return Err(output::report_error(
                    format,
                    "invalid_account_id",
                    "Invalid account ID format",
                ));
            }
        }
    } else {
        None
    };

    // Get days filter if provided
    let days_filter = sub_matches.get_one::<u32>("days").copied();

    // Use the new helper method to get all closed trades
    let mut all_trades = match trust.search_closed_trades(account_id) {
        Ok(trades) => trades,
        Err(_) => {
            return Err(output::report_error(
                    format,
                    "trading_data_unavailable",
                    "Unable to retrieve trading data. Please check your account settings and try again.",
                ));
        }
    };

    // Apply days filter if specified
    if let Some(days) = days_filter {
        all_trades = PerformanceCalculator::filter_trades_by_days(&all_trades, days);
    }

    if format == ReportOutputFormat::Text {
        PerformanceView::display(all_trades);
        return Ok(());
    }

    let stats = PerformanceCalculator::calculate_performance_stats(&all_trades);
    let calculated_win_rate = if stats.total_trades == 0 {
        dec!(0)
    } else {
        Decimal::from(stats.winning_trades)
            .checked_mul(dec!(100))
            .and_then(|v| v.checked_div(Decimal::from(stats.total_trades)))
            .unwrap_or(dec!(0))
    };

    let payload = json!({
        "report": "performance",
        "format_version": 1,
        "generated_at": chrono::Utc::now().to_rfc3339(),
        "scope": { "account_id": account_id.map(|id| id.to_string()) },
        "filters": { "days": days_filter },
        "data": {
            "total_trades": stats.total_trades,
            "winning_trades": stats.winning_trades,
            "losing_trades": stats.losing_trades,
            "win_rate_percentage": output::decimal_string(stats.win_rate),
            "average_win": output::decimal_string(stats.average_win),
            "average_loss": output::decimal_string(stats.average_loss),
            "average_r_multiple": output::decimal_string(stats.average_r_multiple),
            "best_trade": stats.best_trade.map(output::decimal_string),
            "worst_trade": stats.worst_trade.map(output::decimal_string)
        },
        "consistency": {
            "trade_count_balanced": stats.total_trades == stats.winning_trades.saturating_add(stats.losing_trades),
            "win_rate_recomputed_percentage": output::decimal_string(calculated_win_rate),
            "win_rate_delta_percentage": output::decimal_string(
                stats
                    .win_rate
                    .checked_sub(calculated_win_rate)
                    .unwrap_or(dec!(0))
                    .abs()
            )
        }
    });
    output::print_json(&payload)?;

    Ok(())
}
