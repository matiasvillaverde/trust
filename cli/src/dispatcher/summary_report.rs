//! Legacy summary report command responsibility. Business mutations go through TrustFacade.
use crate::dispatcher::concentration_report;
use crate::dispatcher::error::CliError;
use crate::dispatcher::output;
use crate::dispatcher::output::ReportOutputFormat;
use crate::dispatcher::report_calculations;
use crate::dispatcher::report_signals;
use clap::ArgMatches;
use core::TrustFacade;
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use serde_json::json;
use serde_json::Value;
use uuid::Uuid;

#[allow(clippy::too_many_lines)]
pub(in crate::dispatcher) fn summary_report(
    trust: &mut TrustFacade,
    sub_matches: &ArgMatches,
    format: ReportOutputFormat,
) -> Result<(), CliError> {
    use core::calculators_advanced_metrics::AdvancedMetricsCalculator;
    use core::calculators_performance::PerformanceCalculator;
    use core::calculators_risk::CapitalAtRiskCalculator;
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

    // Get trading summary
    let summary = match trust.get_trading_summary(account_id) {
        Ok(summary) => summary,
        Err(e) => {
            return Err(output::report_error(
                format,
                "summary_generation_failed",
                format!("Error generating trading summary: {e}"),
            ));
        }
    };

    let closed_trades = trust
        .search_closed_trades(account_id)
        .unwrap_or_else(|_| Vec::new());
    let performance = summary
        .performance
        .clone()
        .unwrap_or_else(|| PerformanceCalculator::calculate_performance_stats(&closed_trades));
    let total_risk =
        CapitalAtRiskCalculator::calculate_total_capital_at_risk(&summary.capital_at_risk)
            .unwrap_or(dec!(0));
    let risk_pct = if summary.equity > dec!(0) {
        total_risk
            .checked_mul(dec!(100))
            .and_then(|v| v.checked_div(summary.equity))
            .unwrap_or(dec!(0))
    } else {
        dec!(0)
    };

    // Text mode should stay fast and avoid heavy computations that are only used for JSON.
    if format == ReportOutputFormat::Text {
        println!("Trust Trading Summary");
        println!("====================");
        println!(
            "Scope: {}",
            summary
                .account_id
                .map(|id| id.to_string())
                .unwrap_or_else(|| "all_accounts".to_string())
        );
        println!(
            "Equity: {}",
            crate::zen::currency_share(summary.equity, summary.equity)
        );
        println!(
            "Performance: {} trades ({}W/{}L), win rate {:.1}%, avg R {:.2}",
            performance.total_trades,
            performance.winning_trades,
            performance.losing_trades,
            performance.win_rate,
            performance.average_r_multiple
        );
        println!(
            "Risk: {} at risk ({:.2}% of equity), {} open positions",
            crate::zen::currency_share(total_risk, summary.equity),
            risk_pct,
            summary.capital_at_risk.len()
        );
        println!("Concentration groups: {}", summary.concentration.len());
        return Ok(());
    }

    let transactions = report_calculations::transactions_for_scope(trust, summary.account_id, None);
    let drawdown_metrics = report_calculations::drawdown_metrics_for_transactions(&transactions);
    let report_calmar =
        report_calculations::report_calmar_ratio(&closed_trades, &transactions, &drawdown_metrics);
    let profit_concentration =
        AdvancedMetricsCalculator::calculate_profit_concentration_metrics(&closed_trades);

    let advanced = json!({
        "gross_profit": output::decimal_string(AdvancedMetricsCalculator::calculate_gross_profit(&closed_trades)),
        "gross_loss": output::decimal_string(AdvancedMetricsCalculator::calculate_gross_loss(&closed_trades)),
        "net_profit": output::decimal_string(AdvancedMetricsCalculator::calculate_net_profit(&closed_trades)),
        "average_trade_pnl": output::decimal_string(AdvancedMetricsCalculator::calculate_average_trade_pnl(&closed_trades)),
        "median_trade_pnl": AdvancedMetricsCalculator::calculate_median_trade_pnl(&closed_trades).map(output::decimal_string),
        "payoff_ratio": AdvancedMetricsCalculator::calculate_payoff_ratio(&closed_trades).map(output::decimal_string),
        "profit_factor": AdvancedMetricsCalculator::calculate_profit_factor(&closed_trades).map(output::decimal_string),
        "expectancy": output::decimal_string(AdvancedMetricsCalculator::calculate_expectancy(&closed_trades)),
        "sharpe_ratio": AdvancedMetricsCalculator::calculate_sharpe_ratio(&closed_trades, dec!(0.05)).map(output::decimal_string),
        "adjusted_sharpe_ratio": AdvancedMetricsCalculator::calculate_adjusted_sharpe_ratio(&closed_trades, dec!(0.05)).map(output::decimal_string),
        "sortino_ratio": AdvancedMetricsCalculator::calculate_sortino_ratio(&closed_trades, dec!(0.05)).map(output::decimal_string),
        "adjusted_sortino_ratio": AdvancedMetricsCalculator::calculate_adjusted_sortino_ratio(&closed_trades, dec!(0.05)).map(output::decimal_string),
        "calmar_ratio": report_calmar.map(output::decimal_string),
        "value_at_risk_95": AdvancedMetricsCalculator::calculate_value_at_risk(&closed_trades, dec!(0.95)).map(output::decimal_string),
        "expected_shortfall_95": AdvancedMetricsCalculator::calculate_expected_shortfall(&closed_trades, dec!(0.95)).map(output::decimal_string),
        "kelly_criterion": AdvancedMetricsCalculator::calculate_kelly_criterion(&closed_trades).map(output::decimal_string),
        "max_consecutive_wins": AdvancedMetricsCalculator::calculate_max_consecutive_wins(&closed_trades),
        "max_consecutive_losses": AdvancedMetricsCalculator::calculate_max_consecutive_losses(&closed_trades),
        "ulcer_index": AdvancedMetricsCalculator::calculate_ulcer_index(&closed_trades).map(output::decimal_string),
        "risk_of_ruin_proxy_100t_8l": AdvancedMetricsCalculator::calculate_risk_of_ruin_proxy(&closed_trades, 100, 8).map(output::decimal_string),
        "profit_concentration": {
            "top_20pct_profit_share_percentage": output::decimal_string(profit_concentration.top_20pct_profit_share_percentage),
            "trade_share_to_reach_80pct_profit_percentage": output::decimal_string(profit_concentration.trade_share_to_reach_80pct_profit_percentage)
        }
    });
    let rolling = AdvancedMetricsCalculator::calculate_rolling_metrics(
        &closed_trades,
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
    let execution = AdvancedMetricsCalculator::calculate_execution_quality(&closed_trades);
    let journal = AdvancedMetricsCalculator::calculate_journal_quality(&closed_trades);
    let streaks_detail = AdvancedMetricsCalculator::calculate_streak_metrics(&closed_trades);
    let open_trades_for_scope = report_calculations::open_trades_for_scope(trust, account_id);
    let exposure = AdvancedMetricsCalculator::calculate_exposure_metrics(&open_trades_for_scope);
    let bootstrap = AdvancedMetricsCalculator::calculate_bootstrap_confidence_intervals(
        &closed_trades,
        200,
        dec!(0.05),
    );

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
        let per_trade = if closed_trades.is_empty() {
            dec!(0)
        } else {
            total
                .checked_div(Decimal::from(closed_trades.len()))
                .unwrap_or(dec!(0))
        };
        (fee_open, fee_close, total, per_trade)
    };

    let concentration_open_risk_total = summary.concentration.iter().fold(dec!(0), |acc, group| {
        acc.checked_add(group.current_open_risk).unwrap_or(acc)
    });
    let top_concentration_pct = if concentration_open_risk_total > dec!(0) {
        summary
            .concentration
            .iter()
            .map(|group| group.current_open_risk)
            .max()
            .unwrap_or(dec!(0))
            .checked_mul(dec!(100))
            .and_then(|v| v.checked_div(concentration_open_risk_total))
            .unwrap_or(dec!(0))
    } else {
        dec!(0)
    };
    let agent_signals = report_signals::summary_agent_signals(
        risk_pct,
        top_concentration_pct,
        drawdown_metrics.max_drawdown_percentage,
        AdvancedMetricsCalculator::calculate_expectancy(&closed_trades),
    );

    let payload = json!({
        "report": "summary",
        "format_version": 1,
        "generated_at": chrono::Utc::now().to_rfc3339(),
        "scope": { "account_id": summary.account_id.map(|id| id.to_string()) },
        "data": {
            "equity": output::decimal_string(summary.equity),
            "performance": {
                "total_trades": performance.total_trades,
                "winning_trades": performance.winning_trades,
                "losing_trades": performance.losing_trades,
                "win_rate_percentage": output::decimal_string(performance.win_rate),
                "average_r_multiple": output::decimal_string(performance.average_r_multiple),
                "average_win": output::decimal_string(performance.average_win),
                "average_loss": output::decimal_string(performance.average_loss),
                "best_trade": performance.best_trade.map(output::decimal_string),
                "worst_trade": performance.worst_trade.map(output::decimal_string)
            },
            "risk": {
                "total_capital_at_risk": output::decimal_string(total_risk),
                "capital_at_risk_percentage": output::decimal_string(risk_pct),
                "open_positions_count": summary.capital_at_risk.len(),
                "drawdown": {
                    "current_drawdown": output::decimal_string(drawdown_metrics.current_drawdown),
                    "current_drawdown_percentage": output::decimal_string(drawdown_metrics.current_drawdown_percentage),
                    "max_drawdown": output::decimal_string(drawdown_metrics.max_drawdown),
                    "max_drawdown_percentage": output::decimal_string(drawdown_metrics.max_drawdown_percentage),
                    "max_drawdown_date": drawdown_metrics.max_drawdown_date.map(|d| d.format("%Y-%m-%dT%H:%M:%S").to_string())
                }
            },
            "concentration": {
                "group_count": summary.concentration.len(),
                "total_open_risk": output::decimal_string(concentration_open_risk_total),
                "top_groups_by_open_risk": concentration_report::concentration_groups_json(&summary.concentration, concentration_open_risk_total)
            },
            "costs": {
                "fee_open_total": output::decimal_string(fee_open_total),
                "fee_close_total": output::decimal_string(fee_close_total),
                "fees_total": output::decimal_string(fees_total),
                "fees_per_closed_trade": output::decimal_string(fees_per_closed_trade)
            },
            "advanced_metrics": advanced,
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
            "streaks_detail": {
                "max_consecutive_wins": streaks_detail.max_consecutive_wins,
                "max_consecutive_losses": streaks_detail.max_consecutive_losses,
                "average_win_streak": streaks_detail.average_win_streak.map(output::decimal_string),
                "average_loss_streak": streaks_detail.average_loss_streak.map(output::decimal_string),
                "current_streak_type": streaks_detail.current_streak_type,
                "current_streak_len": streaks_detail.current_streak_len
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
        },
        "consistency": {
            "trade_count_balanced": performance.total_trades == performance.winning_trades.saturating_add(performance.losing_trades),
            "risk_is_sum_of_positions": total_risk == summary.capital_at_risk.iter().fold(dec!(0), |acc, p| acc.checked_add(p.capital_amount).unwrap_or(acc))
        }
    });
    output::print_json(&payload)?;

    Ok(())
}
