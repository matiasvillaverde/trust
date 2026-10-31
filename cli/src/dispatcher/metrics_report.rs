//! Legacy metrics report command responsibility. Business mutations go through TrustFacade.
use crate::dispatcher::error::CliError;
use crate::dispatcher::output;
use crate::dispatcher::output::ReportOutputFormat;
use crate::dispatcher::report_calculations;
use crate::dispatcher::report_signals;
use chrono::NaiveDate;
use clap::ArgMatches;
use core::TrustFacade;
use model::TransactionCategory;
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use serde_json::json;
use serde_json::Value;
use uuid::Uuid;

#[allow(clippy::too_many_lines)]
pub(in crate::dispatcher) fn metrics_report(
    trust: &mut TrustFacade,
    sub_matches: &ArgMatches,
    format: ReportOutputFormat,
) -> Result<(), CliError> {
    use crate::views::AdvancedMetricsView;
    use core::calculators_advanced_metrics::AdvancedMetricsCalculator;
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

    // Get all closed trades
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

    let transactions = report_calculations::transactions_for_scope(trust, account_id, days_filter);
    let drawdown_metrics = report_calculations::drawdown_metrics_for_transactions(&transactions);
    let report_calmar =
        report_calculations::report_calmar_ratio(&all_trades, &transactions, &drawdown_metrics);
    let profit_concentration =
        AdvancedMetricsCalculator::calculate_profit_concentration_metrics(&all_trades);

    if format == ReportOutputFormat::Text {
        AdvancedMetricsView::display(
            all_trades,
            Some(crate::views::AdvancedMetricsDisplayContext {
                calmar_ratio: report_calmar,
                risk_free_rate: Some(dec!(0.05)),
            }),
        );
        return Ok(());
    }

    let gross_profit = AdvancedMetricsCalculator::calculate_gross_profit(&all_trades);
    let gross_loss = AdvancedMetricsCalculator::calculate_gross_loss(&all_trades);
    let net_profit = AdvancedMetricsCalculator::calculate_net_profit(&all_trades);
    let average_trade_pnl = AdvancedMetricsCalculator::calculate_average_trade_pnl(&all_trades);
    let median_trade_pnl = AdvancedMetricsCalculator::calculate_median_trade_pnl(&all_trades);

    let profit_factor = AdvancedMetricsCalculator::calculate_profit_factor(&all_trades);
    let expectancy = AdvancedMetricsCalculator::calculate_expectancy(&all_trades);
    let win_rate = AdvancedMetricsCalculator::calculate_win_rate(&all_trades);
    let average_r = AdvancedMetricsCalculator::calculate_average_r_multiple(&all_trades);
    let payoff_ratio = AdvancedMetricsCalculator::calculate_payoff_ratio(&all_trades);
    let sharpe = AdvancedMetricsCalculator::calculate_sharpe_ratio(&all_trades, dec!(0.05));
    let adjusted_sharpe =
        AdvancedMetricsCalculator::calculate_adjusted_sharpe_ratio(&all_trades, dec!(0.05));
    let sortino = AdvancedMetricsCalculator::calculate_sortino_ratio(&all_trades, dec!(0.05));
    let adjusted_sortino =
        AdvancedMetricsCalculator::calculate_adjusted_sortino_ratio(&all_trades, dec!(0.05));
    let calmar = report_calmar;
    let var_95 = AdvancedMetricsCalculator::calculate_value_at_risk(&all_trades, dec!(0.95));
    let es_95 = AdvancedMetricsCalculator::calculate_expected_shortfall(&all_trades, dec!(0.95));
    let kelly = AdvancedMetricsCalculator::calculate_kelly_criterion(&all_trades);
    let max_losses = AdvancedMetricsCalculator::calculate_max_consecutive_losses(&all_trades);
    let max_wins = AdvancedMetricsCalculator::calculate_max_consecutive_wins(&all_trades);
    let ulcer = AdvancedMetricsCalculator::calculate_ulcer_index(&all_trades);
    let risk_of_ruin = AdvancedMetricsCalculator::calculate_risk_of_ruin_proxy(&all_trades, 100, 8);
    let rolling = AdvancedMetricsCalculator::calculate_rolling_metrics(
        &all_trades,
        &[30, 90, 252],
        dec!(0.05),
    );
    let rolling_json: Vec<Value> = rolling
        .iter()
        .map(|window| {
            json!({
                "window_days": window.window_days,
                "trade_count": window.trade_count,
                "sharpe_ratio": window.sharpe_ratio.map(output::decimal_string),
                "sortino_ratio": window.sortino_ratio.map(output::decimal_string),
                "calmar_ratio": window.calmar_ratio.map(output::decimal_string),
                "expectancy": output::decimal_string(window.expectancy),
                "max_drawdown": output::decimal_string(window.max_drawdown)
            })
        })
        .collect();
    let execution = AdvancedMetricsCalculator::calculate_execution_quality(&all_trades);
    let journal = AdvancedMetricsCalculator::calculate_journal_quality(&all_trades);
    let streaks_detail = AdvancedMetricsCalculator::calculate_streak_metrics(&all_trades);
    let open_trades_for_scope = report_calculations::open_trades_for_scope(trust, account_id);
    let exposure = AdvancedMetricsCalculator::calculate_exposure_metrics(&open_trades_for_scope);
    let bootstrap = AdvancedMetricsCalculator::calculate_bootstrap_confidence_intervals(
        &all_trades,
        200,
        dec!(0.05),
    );
    let agent_signals =
        report_signals::metrics_agent_signals(expectancy, win_rate, risk_of_ruin, max_losses);

    let (fee_open_total, fee_close_total, fees_total, fees_per_closed_trade) = {
        let mut fee_open = dec!(0);
        let mut fee_close = dec!(0);
        for tx in &transactions {
            match tx.category {
                model::TransactionCategory::FeeOpen(_) => {
                    fee_open = fee_open.checked_add(tx.amount).unwrap_or(fee_open);
                }
                model::TransactionCategory::FeeClose(_) => {
                    fee_close = fee_close.checked_add(tx.amount).unwrap_or(fee_close);
                }
                _ => {}
            }
        }
        let total = fee_open.checked_add(fee_close).unwrap_or(fee_open);
        let per_trade = if all_trades.is_empty() {
            dec!(0)
        } else {
            total
                .checked_div(Decimal::from(all_trades.len()))
                .unwrap_or(dec!(0))
        };
        (fee_open, fee_close, total, per_trade)
    };

    let payload = json!({
        "report": "metrics",
        "format_version": 1,
        "generated_at": chrono::Utc::now().to_rfc3339(),
        "scope": { "account_id": account_id.map(|id| id.to_string()) },
        "filters": { "days": days_filter },
        "data": {
            "trade_count": all_trades.len(),
            "pnl": {
                "gross_profit": output::decimal_string(gross_profit),
                "gross_loss": output::decimal_string(gross_loss),
                "net_profit": output::decimal_string(net_profit),
                "average_trade_pnl": output::decimal_string(average_trade_pnl),
                "median_trade_pnl": median_trade_pnl.map(output::decimal_string)
            },
            "costs": {
                "fee_open_total": output::decimal_string(fee_open_total),
                "fee_close_total": output::decimal_string(fee_close_total),
                "fees_total": output::decimal_string(fees_total),
                "fees_per_closed_trade": output::decimal_string(fees_per_closed_trade)
            },
            "trade_quality": {
                "profit_factor": profit_factor.map(output::decimal_string),
                "expectancy": output::decimal_string(expectancy),
                "win_rate_percentage": output::decimal_string(win_rate),
                "average_r_multiple": output::decimal_string(average_r),
                "payoff_ratio": payoff_ratio.map(output::decimal_string)
            },
            "profit_concentration": {
                "top_20pct_profit_share_percentage": output::decimal_string(profit_concentration.top_20pct_profit_share_percentage),
                "trade_share_to_reach_80pct_profit_percentage": output::decimal_string(profit_concentration.trade_share_to_reach_80pct_profit_percentage)
            },
            "risk_adjusted_performance": {
                "sharpe_ratio": sharpe.map(output::decimal_string),
                "adjusted_sharpe_ratio": adjusted_sharpe.map(output::decimal_string),
                "sortino_ratio": sortino.map(output::decimal_string),
                "adjusted_sortino_ratio": adjusted_sortino.map(output::decimal_string),
                "calmar_ratio": calmar.map(output::decimal_string),
                "ulcer_index": ulcer.map(output::decimal_string)
            },
            "tail_and_position_sizing": {
                "value_at_risk_95": var_95.map(output::decimal_string),
                "expected_shortfall_95": es_95.map(output::decimal_string),
                "kelly_criterion": kelly.map(output::decimal_string)
            },
            "streaks": {
                "max_consecutive_wins": max_wins,
                "max_consecutive_losses": max_losses
            },
            "streaks_detail": {
                "max_consecutive_wins": streaks_detail.max_consecutive_wins,
                "max_consecutive_losses": streaks_detail.max_consecutive_losses,
                "average_win_streak": streaks_detail.average_win_streak.map(output::decimal_string),
                "average_loss_streak": streaks_detail.average_loss_streak.map(output::decimal_string),
                "current_streak_type": streaks_detail.current_streak_type,
                "current_streak_len": streaks_detail.current_streak_len
            },
            "risk_of_ruin_proxy_100t_8l": risk_of_ruin.map(output::decimal_string),
            "rolling_metrics": rolling_json,
            "execution_quality": {
                "fill_price_coverage_percentage": output::decimal_string(execution.fill_price_coverage_percentage),
                "average_entry_slippage_bps": execution.average_entry_slippage_bps.map(output::decimal_string),
                "average_abs_entry_slippage_bps": execution.average_abs_entry_slippage_bps.map(output::decimal_string),
                "median_entry_slippage_bps": execution.median_entry_slippage_bps.map(output::decimal_string),
                "median_abs_entry_slippage_bps": execution.median_abs_entry_slippage_bps.map(output::decimal_string),
                "p95_abs_entry_slippage_bps": execution.p95_abs_entry_slippage_bps.map(output::decimal_string),
                "stop_fill_price_coverage_percentage": output::decimal_string(execution.stop_fill_price_coverage_percentage),
                "average_stop_slippage_bps": execution.average_stop_slippage_bps.map(output::decimal_string),
                "average_abs_stop_slippage_bps": execution.average_abs_stop_slippage_bps.map(output::decimal_string),
                "target_fill_price_coverage_percentage": output::decimal_string(execution.target_fill_price_coverage_percentage),
                "average_target_slippage_bps": execution.average_target_slippage_bps.map(output::decimal_string),
                "average_abs_target_slippage_bps": execution.average_abs_target_slippage_bps.map(output::decimal_string),
                "average_setup_reward_to_risk": execution.average_setup_reward_to_risk.map(output::decimal_string),
                "average_holding_days": output::decimal_string(execution.average_holding_days),
                "profit_per_holding_day": execution.profit_per_holding_day.map(output::decimal_string)
            },
            "journal_quality": {
                "thesis_coverage_percentage": output::decimal_string(journal.thesis_coverage_percentage),
                "sector_coverage_percentage": output::decimal_string(journal.sector_coverage_percentage),
                "asset_class_coverage_percentage": output::decimal_string(journal.asset_class_coverage_percentage),
                "context_coverage_percentage": output::decimal_string(journal.context_coverage_percentage),
                "complete_journal_percentage": output::decimal_string(journal.complete_journal_percentage)
            },
            "exposure": {
                "gross_exposure": output::decimal_string(exposure.gross_exposure),
                "long_exposure": output::decimal_string(exposure.long_exposure),
                "net_exposure": output::decimal_string(exposure.net_exposure),
                "short_exposure": output::decimal_string(exposure.short_exposure),
                "top_3_symbol_concentration_percentage": output::decimal_string(exposure.top_3_symbol_concentration_percentage),
                "top_sector_concentration_percentage": output::decimal_string(exposure.top_sector_concentration_percentage)
            },
            "confidence_intervals": {
                "expectancy_95": bootstrap.expectancy_95.map(|(low, high)| json!({
                    "low": output::decimal_string(low),
                    "high": output::decimal_string(high)
                })),
                "sharpe_95": bootstrap.sharpe_95.map(|(low, high)| json!({
                    "low": output::decimal_string(low),
                    "high": output::decimal_string(high)
                }))
            },
            "agent_signals": agent_signals
        }
    });
    output::print_json(&payload)?;

    Ok(())
}

pub(in crate::dispatcher) fn parse_required_date_range(
    sub_matches: &ArgMatches,
    format: ReportOutputFormat,
) -> Result<(NaiveDate, NaiveDate), CliError> {
    let from_raw = sub_matches
        .get_one::<String>("from")
        .ok_or_else(|| output::report_error(format, "missing_argument", "Missing --from"))?;
    let to_raw = sub_matches
        .get_one::<String>("to")
        .ok_or_else(|| output::report_error(format, "missing_argument", "Missing --to"))?;
    let from = NaiveDate::parse_from_str(from_raw, "%Y-%m-%d")
        .map_err(|_| output::report_error(format, "invalid_date", "Invalid --from date"))?;
    let to = NaiveDate::parse_from_str(to_raw, "%Y-%m-%d")
        .map_err(|_| output::report_error(format, "invalid_date", "Invalid --to date"))?;
    if from > to {
        return Err(output::report_error(
            format,
            "invalid_date_range",
            "--from must be less than or equal to --to",
        ));
    }
    Ok((from, to))
}

pub(in crate::dispatcher) fn transaction_cashflow_delta(tx: &model::Transaction) -> Decimal {
    match tx.category {
        TransactionCategory::Deposit
        | TransactionCategory::CloseTarget(_)
        | TransactionCategory::CloseSafetyStop(_)
        | TransactionCategory::CloseSafetyStopSlippage(_) => tx.amount,
        TransactionCategory::Withdrawal
        | TransactionCategory::OpenTrade(_)
        | TransactionCategory::FeeOpen(_)
        | TransactionCategory::FeeClose(_)
        | TransactionCategory::WithdrawalTax
        | TransactionCategory::WithdrawalEarnings => {
            Decimal::ZERO.checked_sub(tx.amount).unwrap_or(dec!(0))
        }
        TransactionCategory::FundTrade(_)
        | TransactionCategory::PaymentFromTrade(_)
        | TransactionCategory::PaymentTax(_)
        | TransactionCategory::PaymentEarnings(_) => Decimal::ZERO,
    }
}
