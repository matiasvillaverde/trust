use core::calculators_drawdown::DrawdownMetrics;
use rust_decimal::Decimal;
use rust_decimal_macros::dec;

pub struct DrawdownView;

impl DrawdownView {
    pub fn display(metrics: DrawdownMetrics) {
        println!("\nRealized P&L Drawdown Analysis");
        println!("=============================");

        // Display warning about realized-only limitation
        println!("⚠️  Based on closed trades only - does not include open position losses\n");

        // Display current equity and peak
        println!(
            "Current Account Equity: {}",
            Self::format_currency(metrics.current_equity)
        );
        println!(
            "All-Time Peak Equity: {}",
            Self::format_currency(metrics.peak_equity)
        );

        // Display current drawdown
        if metrics.current_drawdown > dec!(0) {
            println!(
                "Current Drawdown: {} ({})",
                Self::format_currency_negative(metrics.current_drawdown),
                Self::format_percentage_negative(metrics.current_drawdown_percentage)
            );
        } else {
            println!("Current Drawdown: $0.00 (0.0%)");
        }

        println!();

        // Display maximum drawdown
        if metrics.max_drawdown > dec!(0) {
            let date_str = metrics
                .max_drawdown_date
                .map(|d| d.format("%Y-%m-%d").to_string())
                .unwrap_or_else(|| "N/A".to_string());

            println!(
                "Maximum Drawdown: {} ({}) on {}",
                Self::format_currency_negative(metrics.max_drawdown),
                Self::format_percentage_negative(metrics.max_drawdown_percentage),
                date_str
            );

            // Display recovery information
            if metrics.recovery_from_max > dec!(0) {
                println!(
                    "Recovery from Max DD: {} ({} recovered)",
                    Self::format_currency_positive(metrics.recovery_from_max),
                    Self::format_percentage(metrics.recovery_percentage)
                );
            }
        } else {
            println!("Maximum Drawdown: $0.00 (0.0%)");
        }

        // Display days metrics
        if metrics.days_since_peak > 0 {
            println!("Days Since Peak: {}", metrics.days_since_peak);
        }

        if metrics.days_in_drawdown > 0 {
            println!("Days in Current Drawdown: {}", metrics.days_in_drawdown);
        }

        println!();

        // Display drawdown history summary
        Self::display_drawdown_history(&metrics);

        println!();
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

        print!("Peak: {} ", Self::format_currency(metrics.peak_equity));
        print!("→ Low: {} ", Self::format_currency(trough_equity));

        if metrics.max_drawdown > dec!(0) {
            print!(
                "({})",
                Self::format_percentage_negative(metrics.max_drawdown_percentage)
            );
        }

        print!(
            " → Current: {}",
            Self::format_currency(metrics.current_equity)
        );

        if metrics.current_drawdown > dec!(0) && metrics.current_drawdown < metrics.max_drawdown {
            print!(" (partially recovered)");
        } else if metrics.current_drawdown == dec!(0) && metrics.max_drawdown > dec!(0) {
            print!(" (fully recovered)");
        }

        println!();
    }

    fn format_currency(amount: Decimal) -> String {
        if amount >= dec!(0) {
            format!("${amount:.2}")
        } else {
            let abs_amount = amount.abs();
            format!("-${abs_amount:.2}")
        }
    }

    fn format_currency_negative(amount: Decimal) -> String {
        let abs_amount = amount.abs();
        format!("-${abs_amount:.2}")
    }

    fn format_currency_positive(amount: Decimal) -> String {
        let abs_amount = amount.abs();
        format!("+${abs_amount:.2}")
    }

    fn format_percentage(value: Decimal) -> String {
        format!("{value:.1}%")
    }

    fn format_percentage_negative(value: Decimal) -> String {
        let abs_value = value.abs();
        format!("-{abs_value:.1}%")
    }
}
