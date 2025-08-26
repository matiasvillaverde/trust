use core::calculators_advanced_metrics::AdvancedMetricsCalculator;
use model::Trade;
use rust_decimal::Decimal;
use rust_decimal_macros::dec;

pub struct AdvancedMetricsView;

impl AdvancedMetricsView {
    pub fn display(trades: Vec<Trade>) {
        let closed_trades = Self::filter_closed_trades(&trades);

        if closed_trades.is_empty() {
            println!("\nNo closed trades found for the specified criteria.\n");
            return;
        }

        println!("\nAdvanced Trading Metrics");
        println!("=======================");

        Self::display_trade_quality_metrics(&closed_trades);
        println!();
        Self::display_risk_adjusted_metrics(&closed_trades);

        println!();
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

    fn display_trade_quality_metrics(trades: &[Trade]) {
        println!("Trade Quality Metrics:");

        // Profit Factor
        if let Some(profit_factor) = AdvancedMetricsCalculator::calculate_profit_factor(trades) {
            let rating = Self::rate_profit_factor(profit_factor);
            println!("├─ Profit Factor: {:.2} ({})", profit_factor, rating);
        } else {
            println!("├─ Profit Factor: ∞ (Perfect - no losses)");
        }

        // Expectancy
        let expectancy = AdvancedMetricsCalculator::calculate_expectancy(trades);
        let expectancy_rating = if expectancy > dec!(0) {
            "Positive"
        } else {
            "Negative"
        };
        println!(
            "├─ Win Rate: {:.1}% ({})",
            AdvancedMetricsCalculator::calculate_win_rate(trades),
            Self::rate_win_rate(AdvancedMetricsCalculator::calculate_win_rate(trades))
        );
        println!(
            "├─ Average R-Multiple: {:.2}",
            AdvancedMetricsCalculator::calculate_average_r_multiple(trades)
        );
        println!(
            "└─ Expectancy: ${:.2} per trade ({})",
            expectancy, expectancy_rating
        );
    }

    fn display_risk_adjusted_metrics(trades: &[Trade]) {
        println!("Risk-Adjusted Performance:");

        // Use a default risk-free rate of 5% for display purposes
        let risk_free_rate = dec!(0.05);

        // Sharpe Ratio
        if let Some(sharpe) =
            AdvancedMetricsCalculator::calculate_sharpe_ratio(trades, risk_free_rate)
        {
            let rating = Self::rate_sharpe_ratio(sharpe);
            println!("├─ Sharpe Ratio: {:.2} ({})", sharpe, rating);
        } else {
            println!("├─ Sharpe Ratio: N/A (insufficient data)");
        }

        // Sortino Ratio
        if let Some(sortino) =
            AdvancedMetricsCalculator::calculate_sortino_ratio(trades, risk_free_rate)
        {
            let rating = Self::rate_sortino_ratio(sortino);
            println!("├─ Sortino Ratio: {:.2} ({})", sortino, rating);
        } else {
            println!("├─ Sortino Ratio: N/A (insufficient data)");
        }

        // Calmar Ratio
        if let Some(calmar) = AdvancedMetricsCalculator::calculate_calmar_ratio(trades) {
            let rating = Self::rate_calmar_ratio(calmar);
            println!("└─ Calmar Ratio: {:.2} ({})", calmar, rating);
        } else {
            println!("└─ Calmar Ratio: N/A (no drawdown or insufficient data)");
        }
    }

    fn rate_profit_factor(factor: Decimal) -> &'static str {
        if factor >= dec!(3.0) {
            "Excellent"
        } else if factor >= dec!(2.0) {
            "Very Good"
        } else if factor >= dec!(1.5) {
            "Good"
        } else if factor >= dec!(1.0) {
            "Break Even"
        } else {
            "Poor"
        }
    }

    fn rate_win_rate(win_rate: Decimal) -> &'static str {
        if win_rate >= dec!(70.0) {
            "Excellent"
        } else if win_rate >= dec!(60.0) {
            "Very Good"
        } else if win_rate >= dec!(50.0) {
            "Good"
        } else if win_rate >= dec!(40.0) {
            "Fair"
        } else {
            "Poor"
        }
    }

    fn rate_sharpe_ratio(sharpe: Decimal) -> &'static str {
        if sharpe >= dec!(3.0) {
            "Excellent"
        } else if sharpe >= dec!(2.0) {
            "Very Good"
        } else if sharpe >= dec!(1.0) {
            "Good"
        } else if sharpe >= dec!(0.5) {
            "Acceptable"
        } else if sharpe >= dec!(0.0) {
            "Poor"
        } else {
            "Very Poor"
        }
    }

    fn rate_sortino_ratio(sortino: Decimal) -> &'static str {
        if sortino >= dec!(3.0) {
            "Excellent"
        } else if sortino >= dec!(2.0) {
            "Very Good"
        } else if sortino >= dec!(1.0) {
            "Good"
        } else if sortino >= dec!(0.5) {
            "Acceptable"
        } else if sortino >= dec!(0.0) {
            "Poor"
        } else {
            "Very Poor"
        }
    }

    fn rate_calmar_ratio(calmar: Decimal) -> &'static str {
        if calmar >= dec!(3.0) {
            "Excellent"
        } else if calmar >= dec!(2.0) {
            "Very Good"
        } else if calmar >= dec!(1.0) {
            "Good"
        } else if calmar >= dec!(0.5) {
            "Acceptable"
        } else if calmar >= dec!(0.0) {
            "Poor"
        } else {
            "Very Poor"
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use model::trade::{Status, Trade};
    use model::TradeCategory;

    fn create_test_trade(performance: Decimal, status: Status) -> Trade {
        let mut trade = Trade::default();
        trade.balance.total_performance = performance;
        trade.status = status;
        trade.category = TradeCategory::Long;
        trade
    }

    #[test]
    fn test_filter_closed_trades() {
        let trades = vec![
            create_test_trade(dec!(100), Status::ClosedTarget),
            create_test_trade(dec!(-50), Status::ClosedStopLoss),
            create_test_trade(dec!(75), Status::Filled), // Should be filtered out
        ];

        let closed_trades = AdvancedMetricsView::filter_closed_trades(&trades);
        assert_eq!(closed_trades.len(), 2);
        assert!(matches!(closed_trades[0].status, Status::ClosedTarget));
        assert!(matches!(closed_trades[1].status, Status::ClosedStopLoss));
    }

    #[test]
    fn test_rate_profit_factor() {
        assert_eq!(
            AdvancedMetricsView::rate_profit_factor(dec!(3.5)),
            "Excellent"
        );
        assert_eq!(
            AdvancedMetricsView::rate_profit_factor(dec!(2.5)),
            "Very Good"
        );
        assert_eq!(AdvancedMetricsView::rate_profit_factor(dec!(1.7)), "Good");
        assert_eq!(
            AdvancedMetricsView::rate_profit_factor(dec!(1.0)),
            "Break Even"
        );
        assert_eq!(AdvancedMetricsView::rate_profit_factor(dec!(0.8)), "Poor");
    }
}
