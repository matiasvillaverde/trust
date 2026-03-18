use core::calculators_advanced_metrics::{AdvancedMetricsCalculator, ProfitConcentrationMetrics};
use model::Trade;
use rust_decimal::prelude::ToPrimitive;
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use serde_json::json;
use serde_json::Value;

pub struct MetricsExporter;

impl MetricsExporter {
    fn decimal_to_f64(decimal: Option<Decimal>) -> Option<f64> {
        decimal.and_then(|d| d.to_f64())
    }

    fn decimal_to_f64_unwrap(decimal: Decimal) -> f64 {
        decimal.to_f64().unwrap_or(0.0)
    }

    fn push_csv_metric(csv: &mut String, category: &str, name: &str, value: &str, unit: &str) {
        csv.push_str(&format!("{category},{name},{value},{unit}\n"));
    }

    fn push_optional_decimal_csv_metric(
        csv: &mut String,
        category: &str,
        name: &str,
        value: Option<Decimal>,
        unit: &str,
    ) {
        if let Some(value) = value {
            Self::push_csv_metric(csv, category, name, &format!("{value:.4}"), unit);
        }
    }

    fn append_trade_quality_csv_metrics(
        csv: &mut String,
        closed_trades: &[Trade],
        profit_concentration: &ProfitConcentrationMetrics,
    ) {
        Self::push_optional_decimal_csv_metric(
            csv,
            "trade_quality",
            "profit_factor",
            AdvancedMetricsCalculator::calculate_profit_factor(closed_trades),
            "ratio",
        );
        Self::push_csv_metric(
            csv,
            "trade_quality",
            "expectancy",
            &format!(
                "{:.4}",
                AdvancedMetricsCalculator::calculate_expectancy(closed_trades)
            ),
            "currency",
        );
        Self::push_csv_metric(
            csv,
            "trade_quality",
            "win_rate",
            &format!(
                "{:.2}",
                AdvancedMetricsCalculator::calculate_win_rate(closed_trades)
            ),
            "percentage",
        );
        Self::push_csv_metric(
            csv,
            "trade_quality",
            "average_r_multiple",
            &format!(
                "{:.4}",
                AdvancedMetricsCalculator::calculate_average_r_multiple(closed_trades)
            ),
            "ratio",
        );
        Self::push_csv_metric(
            csv,
            "trade_quality",
            "top_20pct_profit_share_percentage",
            &format!(
                "{:.4}",
                profit_concentration.top_20pct_profit_share_percentage
            ),
            "percentage",
        );
        Self::push_csv_metric(
            csv,
            "trade_quality",
            "trade_share_to_reach_80pct_profit_percentage",
            &format!(
                "{:.4}",
                profit_concentration.trade_share_to_reach_80pct_profit_percentage
            ),
            "percentage",
        );
    }

    fn append_risk_adjusted_csv_metrics(
        csv: &mut String,
        closed_trades: &[Trade],
        risk_free: Decimal,
    ) {
        Self::push_optional_decimal_csv_metric(
            csv,
            "risk_adjusted",
            "sharpe_ratio",
            AdvancedMetricsCalculator::calculate_sharpe_ratio(closed_trades, risk_free),
            "ratio",
        );
        Self::push_optional_decimal_csv_metric(
            csv,
            "risk_adjusted",
            "adjusted_sharpe_ratio",
            AdvancedMetricsCalculator::calculate_adjusted_sharpe_ratio(closed_trades, risk_free),
            "ratio",
        );
        Self::push_optional_decimal_csv_metric(
            csv,
            "risk_adjusted",
            "sortino_ratio",
            AdvancedMetricsCalculator::calculate_sortino_ratio(closed_trades, risk_free),
            "ratio",
        );
        Self::push_optional_decimal_csv_metric(
            csv,
            "risk_adjusted",
            "adjusted_sortino_ratio",
            AdvancedMetricsCalculator::calculate_adjusted_sortino_ratio(closed_trades, risk_free),
            "ratio",
        );
        Self::push_optional_decimal_csv_metric(
            csv,
            "risk_adjusted",
            "calmar_ratio",
            AdvancedMetricsCalculator::calculate_calmar_ratio(closed_trades),
            "ratio",
        );
    }

    fn append_statistical_csv_metrics(csv: &mut String, closed_trades: &[Trade]) {
        Self::push_optional_decimal_csv_metric(
            csv,
            "statistical",
            "value_at_risk_95",
            AdvancedMetricsCalculator::calculate_value_at_risk(closed_trades, dec!(0.95)),
            "percentage",
        );
        Self::push_optional_decimal_csv_metric(
            csv,
            "statistical",
            "kelly_criterion",
            AdvancedMetricsCalculator::calculate_kelly_criterion(closed_trades),
            "ratio",
        );
        Self::push_csv_metric(
            csv,
            "statistical",
            "max_consecutive_losses",
            &AdvancedMetricsCalculator::calculate_max_consecutive_losses(closed_trades).to_string(),
            "count",
        );
        Self::push_csv_metric(
            csv,
            "statistical",
            "max_consecutive_wins",
            &AdvancedMetricsCalculator::calculate_max_consecutive_wins(closed_trades).to_string(),
            "count",
        );
        Self::push_optional_decimal_csv_metric(
            csv,
            "statistical",
            "ulcer_index",
            AdvancedMetricsCalculator::calculate_ulcer_index(closed_trades),
            "percentage",
        );
    }

    /// Export advanced metrics to JSON format
    pub fn to_json(trades: &[Trade], risk_free_rate: Option<Decimal>) -> Value {
        let risk_free = risk_free_rate.unwrap_or(dec!(0.05));
        let closed_trades = Self::filter_closed_trades(trades);
        let profit_concentration =
            AdvancedMetricsCalculator::calculate_profit_concentration_metrics(&closed_trades);

        json!({
            "metadata": {
                "total_trades": trades.len(),
                "closed_trades": closed_trades.len(),
                "risk_free_rate": risk_free.to_f64().unwrap_or(0.05),
                "export_timestamp": chrono::Utc::now().to_rfc3339()
            },
            "trade_quality_metrics": {
                "profit_factor": Self::decimal_to_f64(AdvancedMetricsCalculator::calculate_profit_factor(&closed_trades)),
                "expectancy": Self::decimal_to_f64_unwrap(AdvancedMetricsCalculator::calculate_expectancy(&closed_trades)),
                "win_rate": Self::decimal_to_f64_unwrap(AdvancedMetricsCalculator::calculate_win_rate(&closed_trades)),
                "average_r_multiple": Self::decimal_to_f64_unwrap(AdvancedMetricsCalculator::calculate_average_r_multiple(&closed_trades))
            },
            "profit_concentration": {
                "top_20pct_profit_share_percentage": Self::decimal_to_f64_unwrap(profit_concentration.top_20pct_profit_share_percentage),
                "trade_share_to_reach_80pct_profit_percentage": Self::decimal_to_f64_unwrap(profit_concentration.trade_share_to_reach_80pct_profit_percentage)
            },
            "risk_adjusted_performance": {
                "sharpe_ratio": Self::decimal_to_f64(AdvancedMetricsCalculator::calculate_sharpe_ratio(&closed_trades, risk_free)),
                "adjusted_sharpe_ratio": Self::decimal_to_f64(AdvancedMetricsCalculator::calculate_adjusted_sharpe_ratio(&closed_trades, risk_free)),
                "sortino_ratio": Self::decimal_to_f64(AdvancedMetricsCalculator::calculate_sortino_ratio(&closed_trades, risk_free)),
                "adjusted_sortino_ratio": Self::decimal_to_f64(AdvancedMetricsCalculator::calculate_adjusted_sortino_ratio(&closed_trades, risk_free)),
                "calmar_ratio": Self::decimal_to_f64(AdvancedMetricsCalculator::calculate_calmar_ratio(&closed_trades))
            },
            "statistical_analysis": {
                "value_at_risk_95": Self::decimal_to_f64(AdvancedMetricsCalculator::calculate_value_at_risk(&closed_trades, dec!(0.95))),
                "kelly_criterion": Self::decimal_to_f64(AdvancedMetricsCalculator::calculate_kelly_criterion(&closed_trades)),
                "max_consecutive_losses": AdvancedMetricsCalculator::calculate_max_consecutive_losses(&closed_trades),
                "max_consecutive_wins": AdvancedMetricsCalculator::calculate_max_consecutive_wins(&closed_trades),
                "ulcer_index": Self::decimal_to_f64(AdvancedMetricsCalculator::calculate_ulcer_index(&closed_trades))
            }
        })
    }

    /// Export advanced metrics to CSV format
    pub fn to_csv(trades: &[Trade], risk_free_rate: Option<Decimal>) -> String {
        let risk_free = risk_free_rate.unwrap_or(dec!(0.05));
        let closed_trades = Self::filter_closed_trades(trades);
        let profit_concentration =
            AdvancedMetricsCalculator::calculate_profit_concentration_metrics(&closed_trades);

        let mut csv = String::new();
        csv.push_str("metric_category,metric_name,value,unit\n");

        Self::append_trade_quality_csv_metrics(&mut csv, &closed_trades, &profit_concentration);
        Self::append_risk_adjusted_csv_metrics(&mut csv, &closed_trades, risk_free);
        Self::append_statistical_csv_metrics(&mut csv, &closed_trades);

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

        assert_eq!(
            result.get("metadata").unwrap().get("total_trades").unwrap(),
            0
        );
        assert_eq!(
            result
                .get("metadata")
                .unwrap()
                .get("closed_trades")
                .unwrap(),
            0
        );
    }

    #[test]
    fn test_json_export_with_trades() {
        let trades = vec![create_test_trade(dec!(100)), create_test_trade(dec!(-50))];
        let result = MetricsExporter::to_json(&trades, Some(dec!(0.05)));

        assert_eq!(
            result.get("metadata").unwrap().get("total_trades").unwrap(),
            2
        );
        assert_eq!(
            result
                .get("metadata")
                .unwrap()
                .get("closed_trades")
                .unwrap(),
            2
        );
        assert_eq!(
            result
                .get("metadata")
                .unwrap()
                .get("risk_free_rate")
                .unwrap(),
            0.05
        );
        assert!(result
            .get("trade_quality_metrics")
            .unwrap()
            .get("expectancy")
            .unwrap()
            .is_number());
        assert!(result
            .get("profit_concentration")
            .unwrap()
            .get("top_20pct_profit_share_percentage")
            .unwrap()
            .is_number());
        let risk_adjusted_performance = result.get("risk_adjusted_performance").unwrap();
        assert!(risk_adjusted_performance
            .get("adjusted_sharpe_ratio")
            .is_some());
        assert!(risk_adjusted_performance
            .get("adjusted_sortino_ratio")
            .is_some());
    }

    #[test]
    fn test_csv_export_format() {
        let trades = vec![
            create_test_trade(dec!(100)),
            create_test_trade(dec!(100)),
            create_test_trade(dec!(100)),
            create_test_trade(dec!(-50)),
            create_test_trade(dec!(-25)),
        ];
        let result = MetricsExporter::to_csv(&trades, None);

        assert!(result.starts_with("metric_category,metric_name,value,unit\n"));
        assert!(result.contains("trade_quality,expectancy,"));
        assert!(result.contains("trade_quality,top_20pct_profit_share_percentage,"));
        assert!(result.contains("statistical,max_consecutive"));
    }
}
