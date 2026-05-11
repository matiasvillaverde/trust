use core::calculators_drawdown::DrawdownMetrics;
use rust_decimal::Decimal;
use rust_decimal_macros::dec;

pub struct DrawdownView;

impl DrawdownView {
    pub fn display(metrics: DrawdownMetrics) {
        println!("\nRealized P&L Drawdown Analysis");
        println!("=============================");
        println!("⚠️  Based on closed trades only - does not include open position losses\n");

        Self::display_equity_header(&metrics);
        Self::display_current_drawdown(&metrics);
        println!();
        Self::display_max_drawdown(&metrics);
        Self::display_day_metrics(&metrics);
        println!();
        Self::display_drawdown_history(&metrics);
        println!();
    }

    fn display_equity_header(metrics: &DrawdownMetrics) {
        println!(
            "Current Account Equity: {}",
            Self::format_equity(metrics.current_equity, metrics.peak_equity)
        );
        println!(
            "All-Time Peak Equity: {}",
            Self::format_equity(metrics.peak_equity, metrics.peak_equity)
        );
    }

    fn display_current_drawdown(metrics: &DrawdownMetrics) {
        if metrics.current_drawdown > dec!(0) {
            if crate::zen::is_enabled() {
                println!(
                    "Current Drawdown: {}",
                    Self::format_percentage_negative(metrics.current_drawdown_percentage)
                );
            } else {
                println!(
                    "Current Drawdown: {} ({})",
                    Self::format_currency_negative(metrics.current_drawdown),
                    Self::format_percentage_negative(metrics.current_drawdown_percentage)
                );
            }
        } else {
            println!("Current Drawdown: {}", Self::zero_drawdown_label());
        }
    }

    fn display_max_drawdown(metrics: &DrawdownMetrics) {
        if metrics.max_drawdown > dec!(0) {
            Self::display_nonzero_max_drawdown(metrics);
        } else {
            println!("Maximum Drawdown: {}", Self::zero_drawdown_label());
        }
    }

    fn display_nonzero_max_drawdown(metrics: &DrawdownMetrics) {
        let date_str = metrics
            .max_drawdown_date
            .map(|d| d.format("%Y-%m-%d").to_string())
            .unwrap_or_else(|| "N/A".to_string());

        if crate::zen::is_enabled() {
            println!(
                "Maximum Drawdown: {} on {}",
                Self::format_percentage_negative(metrics.max_drawdown_percentage),
                date_str
            );
        } else {
            println!(
                "Maximum Drawdown: {} ({}) on {}",
                Self::format_currency_negative(metrics.max_drawdown),
                Self::format_percentage_negative(metrics.max_drawdown_percentage),
                date_str
            );
        }
        Self::display_recovery_from_max(metrics);
    }

    fn display_recovery_from_max(metrics: &DrawdownMetrics) {
        if metrics.recovery_from_max <= dec!(0) {
            return;
        }

        if crate::zen::is_enabled() {
            println!(
                "Recovery from Max DD: {} recovered",
                Self::format_percentage(metrics.recovery_percentage)
            );
        } else {
            println!(
                "Recovery from Max DD: {} ({} recovered)",
                Self::format_currency_positive(metrics.recovery_from_max),
                Self::format_percentage(metrics.recovery_percentage)
            );
        }
    }

    fn display_day_metrics(metrics: &DrawdownMetrics) {
        if metrics.days_since_peak > 0 {
            println!("Days Since Peak: {}", metrics.days_since_peak);
        }

        if metrics.days_in_drawdown > 0 {
            println!("Days in Current Drawdown: {}", metrics.days_in_drawdown);
        }
    }

    fn zero_drawdown_label() -> &'static str {
        if crate::zen::is_enabled() {
            "0.0%"
        } else {
            "$0.00 (0.0%)"
        }
    }

    fn display_drawdown_history(metrics: &DrawdownMetrics) {
        println!("Drawdown History Summary:");

        if metrics.max_drawdown == dec!(0) {
            println!("No drawdowns recorded - account has only experienced gains");
            return;
        }

        // Show peak → trough → current progression
        let trough_equity = metrics
            .peak_equity
            .checked_sub(metrics.max_drawdown)
            .unwrap_or(dec!(0));

        print!(
            "Peak: {} ",
            Self::format_equity(metrics.peak_equity, metrics.peak_equity)
        );
        print!(
            "→ Low: {} ",
            Self::format_equity(trough_equity, metrics.peak_equity)
        );

        if metrics.max_drawdown > dec!(0) {
            print!(
                "({})",
                Self::format_percentage_negative(metrics.max_drawdown_percentage)
            );
        }

        print!(
            " → Current: {}",
            Self::format_equity(metrics.current_equity, metrics.peak_equity)
        );

        if metrics.current_drawdown > dec!(0) && metrics.current_drawdown < metrics.max_drawdown {
            print!(" (partially recovered)");
        } else if metrics.current_drawdown == dec!(0) && metrics.max_drawdown > dec!(0) {
            print!(" (fully recovered)");
        }

        println!();
    }

    fn format_currency(amount: Decimal) -> String {
        crate::zen::currency(amount)
    }

    fn format_currency_negative(amount: Decimal) -> String {
        if crate::zen::is_enabled() {
            "hidden".to_string()
        } else {
            let abs_amount = amount.abs();
            format!("-${abs_amount:.2}")
        }
    }

    fn format_currency_positive(amount: Decimal) -> String {
        crate::zen::signed_currency(amount.abs())
    }

    fn format_equity(amount: Decimal, basis: Decimal) -> String {
        if crate::zen::is_enabled() {
            crate::zen::percentage_of(amount, basis)
        } else {
            Self::format_currency(amount)
        }
    }

    fn format_percentage(value: Decimal) -> String {
        format!("{value:.1}%")
    }

    fn format_percentage_negative(value: Decimal) -> String {
        let abs_value = value.abs();
        format!("-{abs_value:.1}%")
    }
}

#[cfg(test)]
mod tests {
    use super::DrawdownView;
    use chrono::Utc;
    use core::calculators_drawdown::DrawdownMetrics;
    use rust_decimal_macros::dec;

    fn sample_metrics() -> DrawdownMetrics {
        DrawdownMetrics {
            current_equity: dec!(9800),
            peak_equity: dec!(10000),
            current_drawdown: dec!(200),
            current_drawdown_percentage: dec!(2.0),
            max_drawdown: dec!(500),
            max_drawdown_percentage: dec!(5.0),
            max_drawdown_date: Some(Utc::now().naive_utc()),
            recovery_from_max: dec!(300),
            recovery_percentage: dec!(60.0),
            days_since_peak: 10,
            days_in_drawdown: 10,
        }
    }

    #[test]
    fn currency_and_percentage_formatters_are_consistent() {
        crate::zen::set_enabled(false);
        assert_eq!(DrawdownView::format_currency(dec!(10)), "$10.00");
        assert_eq!(DrawdownView::format_currency(dec!(-10)), "-$10.00");
        assert_eq!(DrawdownView::format_currency_negative(dec!(10)), "-$10.00");
        assert_eq!(DrawdownView::format_currency_positive(dec!(10)), "+$10.00");
        assert_eq!(DrawdownView::format_percentage(dec!(12.34)), "12.3%");
        assert_eq!(
            DrawdownView::format_percentage_negative(dec!(12.34)),
            "-12.3%"
        );
    }

    #[test]
    fn equity_formatter_uses_peak_percentages_in_zen_mode() {
        crate::zen::set_enabled(true);
        assert_eq!(
            DrawdownView::format_equity(dec!(9500), dec!(10000)),
            "95.0%"
        );
        assert_eq!(DrawdownView::format_currency(dec!(10)), "hidden");
        crate::zen::set_enabled(false);
    }

    #[test]
    fn display_and_history_cover_recovery_and_no_drawdown_paths() {
        crate::zen::set_enabled(false);
        let metrics = sample_metrics();
        DrawdownView::display_drawdown_history(&metrics);
        DrawdownView::display(metrics);

        let no_drawdown = DrawdownMetrics {
            current_drawdown: dec!(0),
            max_drawdown: dec!(0),
            ..sample_metrics()
        };
        DrawdownView::display_drawdown_history(&no_drawdown);
        DrawdownView::display(no_drawdown);

        let fully_recovered = DrawdownMetrics {
            current_equity: dec!(10000),
            current_drawdown: dec!(0),
            max_drawdown: dec!(500),
            ..sample_metrics()
        };
        DrawdownView::display_drawdown_history(&fully_recovered);
    }

    #[test]
    fn display_handles_missing_max_date_zero_recovery_and_no_day_counts() {
        crate::zen::set_enabled(false);
        let metrics = DrawdownMetrics {
            current_equity: dec!(9500),
            peak_equity: dec!(10000),
            current_drawdown: dec!(500),
            current_drawdown_percentage: dec!(5.0),
            max_drawdown: dec!(500),
            max_drawdown_percentage: dec!(5.0),
            max_drawdown_date: None,
            recovery_from_max: dec!(0),
            recovery_percentage: dec!(0),
            days_since_peak: 0,
            days_in_drawdown: 0,
        };

        DrawdownView::display_drawdown_history(&metrics);
        DrawdownView::display(metrics);
    }
}
