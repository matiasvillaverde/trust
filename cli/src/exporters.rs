use core::calculators_advanced_metrics::AdvancedMetricsCalculator;
use model::Trade;
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use serde_json::json;
use serde_json::Value;

pub struct MetricsExporter;

impl MetricsExporter {
    /// Export advanced metrics to JSON format
    pub fn to_json(trades: &[Trade], risk_free_rate: Option<Decimal>) -> Value {
        let risk_free = risk_free_rate.unwrap_or(dec!(0.05));
        let closed_trades = Self::filter_closed_trades(trades);

        json!({
            "metadata": {
                "total_trades": trades.len(),
                "closed_trades": closed_trades.len(),
                "risk_free_rate": risk_free,
                "export_timestamp": chrono::Utc::now().to_rfc3339()
            },
            "trade_quality_metrics": {
                "profit_factor": AdvancedMetricsCalculator::calculate_profit_factor(&closed_trades),
                "expectancy": AdvancedMetricsCalculator::calculate_expectancy(&closed_trades),
                "win_rate": AdvancedMetricsCalculator::calculate_win_rate(&closed_trades),
                "average_r_multiple": AdvancedMetricsCalculator::calculate_average_r_multiple(&closed_trades)
            },
            "risk_adjusted_performance": {
                "sharpe_ratio": AdvancedMetricsCalculator::calculate_sharpe_ratio(&closed_trades, risk_free),
                "sortino_ratio": AdvancedMetricsCalculator::calculate_sortino_ratio(&closed_trades, risk_free),
                "calmar_ratio": AdvancedMetricsCalculator::calculate_calmar_ratio(&closed_trades)
            },
            "statistical_analysis": {
                "value_at_risk_95": AdvancedMetricsCalculator::calculate_value_at_risk(&closed_trades, dec!(0.95)),
                "kelly_criterion": AdvancedMetricsCalculator::calculate_kelly_criterion(&closed_trades),
                "max_consecutive_losses": AdvancedMetricsCalculator::calculate_max_consecutive_losses(&closed_trades),
                "max_consecutive_wins": AdvancedMetricsCalculator::calculate_max_consecutive_wins(&closed_trades),
                "ulcer_index": AdvancedMetricsCalculator::calculate_ulcer_index(&closed_trades)
            }
        })
    }

    /// Export advanced metrics to CSV format
    pub fn to_csv(trades: &[Trade], risk_free_rate: Option<Decimal>) -> String {
        let risk_free = risk_free_rate.unwrap_or(dec!(0.05));
        let closed_trades = Self::filter_closed_trades(trades);

        let mut csv = String::new();
        csv.push_str("metric_category,metric_name,value,unit\n");

        // Trade Quality Metrics
        if let Some(pf) = AdvancedMetricsCalculator::calculate_profit_factor(&closed_trades) {
            csv.push_str(&format!("trade_quality,profit_factor,{:.4},ratio\n", pf));
        }

        let expectancy = AdvancedMetricsCalculator::calculate_expectancy(&closed_trades);
        csv.push_str(&format!(
            "trade_quality,expectancy,{:.4},currency\n",
            expectancy
        ));

        let win_rate = AdvancedMetricsCalculator::calculate_win_rate(&closed_trades);
        csv.push_str(&format!(
            "trade_quality,win_rate,{:.2},percentage\n",
            win_rate
        ));

        let avg_r = AdvancedMetricsCalculator::calculate_average_r_multiple(&closed_trades);
        csv.push_str(&format!(
            "trade_quality,average_r_multiple,{:.4},ratio\n",
            avg_r
        ));

        // Risk-Adjusted Performance
        if let Some(sharpe) =
            AdvancedMetricsCalculator::calculate_sharpe_ratio(&closed_trades, risk_free)
        {
            csv.push_str(&format!("risk_adjusted,sharpe_ratio,{:.4},ratio\n", sharpe));
        }

        if let Some(sortino) =
            AdvancedMetricsCalculator::calculate_sortino_ratio(&closed_trades, risk_free)
        {
            csv.push_str(&format!(
                "risk_adjusted,sortino_ratio,{:.4},ratio\n",
                sortino
            ));
        }

        if let Some(calmar) = AdvancedMetricsCalculator::calculate_calmar_ratio(&closed_trades) {
            csv.push_str(&format!("risk_adjusted,calmar_ratio,{:.4},ratio\n", calmar));
        }

        // Statistical Analysis
        if let Some(var) =
            AdvancedMetricsCalculator::calculate_value_at_risk(&closed_trades, dec!(0.95))
        {
            csv.push_str(&format!(
                "statistical,value_at_risk_95,{:.4},percentage\n",
                var
            ));
        }

        if let Some(kelly) = AdvancedMetricsCalculator::calculate_kelly_criterion(&closed_trades) {
            csv.push_str(&format!("statistical,kelly_criterion,{:.4},ratio\n", kelly));
        }

        let max_losses =
            AdvancedMetricsCalculator::calculate_max_consecutive_losses(&closed_trades);
        csv.push_str(&format!(
            "statistical,max_consecutive_losses,{},count\n",
            max_losses
        ));

        let max_wins = AdvancedMetricsCalculator::calculate_max_consecutive_wins(&closed_trades);
        csv.push_str(&format!(
            "statistical,max_consecutive_wins,{},count\n",
            max_wins
        ));

        if let Some(ulcer) = AdvancedMetricsCalculator::calculate_ulcer_index(&closed_trades) {
            csv.push_str(&format!(
                "statistical,ulcer_index,{:.4},percentage\n",
                ulcer
            ));
        }

        csv
    }

    fn filter_closed_trades(trades: &[Trade]) -> Vec<Trade> {
        trades
            .iter()
            .filter(|trade| {
                matches!(
                    trade.status,
                    model::Status::ClosedTarget | model::Status::ClosedStopLoss
                )
            })
            .cloned()
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use model::trade::{Status, Trade};
    use model::TradeCategory;

    fn create_test_trade(performance: Decimal) -> Trade {
        let mut trade = Trade::default();
        trade.balance.total_performance = performance;
        trade.status = Status::ClosedTarget;
        trade.category = TradeCategory::Long;
        trade
    }

    #[test]
    fn test_json_export_empty_trades() {
        let trades = vec![];
        let result = MetricsExporter::to_json(&trades, None);

        assert_eq!(result["metadata"]["total_trades"], 0);
        assert_eq!(result["metadata"]["closed_trades"], 0);
    }

    #[test]
    fn test_json_export_with_trades() {
        let trades = vec![create_test_trade(dec!(100)), create_test_trade(dec!(-50))];
        let result = MetricsExporter::to_json(&trades, Some(dec!(0.05)));

        assert_eq!(result["metadata"]["total_trades"], 2);
        assert_eq!(result["metadata"]["closed_trades"], 2);
        assert_eq!(result["metadata"]["risk_free_rate"], dec!(0.05));
        assert!(result["trade_quality_metrics"]["expectancy"].is_number());
    }

    #[test]
    fn test_csv_export_format() {
        let trades = vec![create_test_trade(dec!(100)), create_test_trade(dec!(-50))];
        let result = MetricsExporter::to_csv(&trades, None);

        assert!(result.starts_with("metric_category,metric_name,value,unit\n"));
        assert!(result.contains("trade_quality,expectancy,"));
        assert!(result.contains("statistical,max_consecutive"));
    }
}
