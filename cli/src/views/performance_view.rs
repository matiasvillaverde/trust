use core::calculators_performance::{PerformanceCalculator, PerformanceStats};
use model::Trade;
use rust_decimal::Decimal;
use rust_decimal_macros::dec;

pub struct PerformanceView;

impl PerformanceView {
    pub fn display(trades: Vec<Trade>) {
        let closed_trades = PerformanceCalculator::filter_closed_trades(&trades);

        if closed_trades.is_empty() {
            println!("\nNo closed trades found for the specified criteria.\n");
            return;
        }

        let stats = PerformanceCalculator::calculate_performance_stats(&closed_trades);

        println!("\nTrading Performance Report");
        println!("========================");

        Self::display_trade_summary(&stats);
        Self::display_win_loss_analysis(&stats);
        Self::display_performance_metrics(&stats);

        println!();
    }

    fn display_trade_summary(stats: &PerformanceStats) {
        println!("Total Trades: {}", stats.total_trades);
        println!(
            "Winning Trades: {} ({:.1}%)",
            stats.winning_trades, stats.win_rate
        );
        const HUNDRED_PERCENT: Decimal = dec!(100);
        let losing_percentage = HUNDRED_PERCENT
            .checked_sub(stats.win_rate)
            .unwrap_or(dec!(0));
        println!(
            "Losing Trades: {} ({:.1}%)",
            stats.losing_trades, losing_percentage
        );
    }

    fn display_win_loss_analysis(stats: &PerformanceStats) {
        let avg_win_display = if stats.average_win > Decimal::ZERO {
            format!("${:.2}", stats.average_win)
        } else {
            "$0.00".to_string()
        };

        let avg_loss_display = if stats.average_loss < Decimal::ZERO {
            format!("${:.2}", stats.average_loss)
        } else {
            "$0.00".to_string()
        };

        println!("Average Win: {avg_win_display}");
        println!("Average Loss: {avg_loss_display}");
    }

    fn display_performance_metrics(stats: &PerformanceStats) {
        println!("Average R-Multiple: {:.2}", stats.average_r_multiple);

        if let Some(best) = stats.best_trade {
            println!("Best Trade: ${best:.2}");
        }

        if let Some(worst) = stats.worst_trade {
            println!("Worst Trade: ${worst:.2}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::PerformanceView;
    use core::calculators_performance::PerformanceStats;
    use model::{Status, Trade};
    use rust_decimal_macros::dec;

    fn stats() -> PerformanceStats {
        PerformanceStats {
            total_trades: 10,
            winning_trades: 6,
            losing_trades: 4,
            win_rate: dec!(60),
            average_win: dec!(120.50),
            average_loss: dec!(-80.25),
            best_trade: Some(dec!(300)),
            worst_trade: Some(dec!(-150)),
            average_r_multiple: dec!(1.25),
        }
    }

    #[test]
    fn display_helpers_handle_positive_and_zero_edge_cases() {
        let mut s = stats();
        PerformanceView::display_trade_summary(&s);
        PerformanceView::display_win_loss_analysis(&s);
        PerformanceView::display_performance_metrics(&s);

        s.average_win = dec!(0);
        s.average_loss = dec!(0);
        s.best_trade = None;
        s.worst_trade = None;
        PerformanceView::display_win_loss_analysis(&s);
        PerformanceView::display_performance_metrics(&s);
    }

    #[test]
    fn display_handles_empty_and_closed_trade_inputs() {
        PerformanceView::display(Vec::new());

        let mut trade = Trade {
            status: Status::ClosedTarget,
            ..Trade::default()
        };
        trade.balance.total_performance = dec!(100);
        trade.balance.funding = dec!(1000);
        trade.balance.capital_in_market = dec!(0);
        PerformanceView::display(vec![trade]);
    }
}
