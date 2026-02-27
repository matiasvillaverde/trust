use core::calculators_risk::OpenPosition;
use rust_decimal::Decimal;
use rust_decimal_macros::dec;

pub struct RiskView;

impl RiskView {
    pub fn display(
        positions: Vec<OpenPosition>,
        total_capital_at_risk: Decimal,
        account_equity: Decimal,
    ) {
        println!("\nCapital at Risk Analysis");
        println!("=========================");

        // Display account equity
        println!(
            "Total Account Equity: {}",
            Self::format_currency(account_equity)
        );

        // Display capital at risk
        println!(
            "Capital at Risk: {} ({}% of account)",
            Self::format_currency(total_capital_at_risk),
            Self::calculate_percentage(total_capital_at_risk, account_equity)
        );

        // Display safe capital
        let safe_capital = account_equity
            .checked_sub(total_capital_at_risk)
            .unwrap_or(dec!(0));
        println!(
            "Safe Capital: {} ({}% of account)",
            Self::format_currency(safe_capital),
            Self::calculate_percentage(safe_capital, account_equity)
        );

        // Display open positions if any
        if !positions.is_empty() {
            println!("\nOpen Positions:");
            for position in positions {
                let position_percentage =
                    Self::calculate_percentage(position.capital_amount, account_equity);
                let funded_date = position.funded_date.format("%Y-%m-%d").to_string();

                println!(
                    "{}: {} ({}%) - Funded {}, {}",
                    position.symbol,
                    Self::format_currency(position.capital_amount),
                    position_percentage,
                    funded_date,
                    Self::format_status(&position.status)
                );
            }
        } else {
            println!("\nNo open positions");
        }

        // Display risk status
        println!();
        Self::display_risk_status(total_capital_at_risk, account_equity);
    }

    fn format_currency(amount: Decimal) -> String {
        if amount >= dec!(0) {
            format!("${amount:.2}")
        } else {
            format!("-${:.2}", amount.abs())
        }
    }

    fn calculate_percentage(amount: Decimal, total: Decimal) -> String {
        if total == dec!(0) {
            "0.0".to_string()
        } else {
            let percentage = amount
                .checked_mul(dec!(100))
                .and_then(|v| v.checked_div(total))
                .unwrap_or(dec!(0));
            format!("{percentage:.1}")
        }
    }

    fn format_status(status: &model::Status) -> &str {
        match status {
            model::Status::Funded => "Entry pending",
            model::Status::Submitted => "Order submitted",
            model::Status::Filled => "Target/Stop active",
            _ => "Unknown status",
        }
    }

    fn display_risk_status(capital_at_risk: Decimal, account_equity: Decimal) {
        if account_equity == dec!(0) {
            println!("Risk Status: ⚠️  NO ACCOUNT EQUITY");
            return;
        }

        let risk_percentage = capital_at_risk
            .checked_mul(dec!(100))
            .and_then(|v| v.checked_div(account_equity))
            .unwrap_or(dec!(0));

        let (icon, status, color_code) = if risk_percentage < dec!(10) {
            ("✅", "HEALTHY", "\x1b[32m") // Green
        } else if risk_percentage < dec!(20) {
            ("⚠️", "WARNING", "\x1b[33m") // Yellow
        } else {
            ("🔴", "HIGH RISK", "\x1b[31m") // Red
        };

        // Reset color code
        let reset = "\x1b[0m";

        println!(
            "Risk Status: {} {}{}{} ({}% risk threshold)",
            icon,
            color_code,
            status,
            reset,
            if risk_percentage < dec!(10) {
                "Below 10%"
            } else if risk_percentage < dec!(20) {
                "10-20%"
            } else {
                "Above 20%"
            }
        );
    }
}

#[cfg(test)]
mod tests {
    use super::RiskView;
    use chrono::Utc;
    use core::calculators_risk::OpenPosition;
    use model::Status;
    use rust_decimal_macros::dec;
    use uuid::Uuid;

    #[test]
    fn format_currency_covers_positive_and_negative_values() {
        assert_eq!(RiskView::format_currency(dec!(1500.5)), "$1500.50");
        assert_eq!(RiskView::format_currency(dec!(-1500.5)), "-$1500.50");
    }

    #[test]
    fn calculate_percentage_handles_zero_and_regular_totals() {
        assert_eq!(RiskView::calculate_percentage(dec!(10), dec!(0)), "0.0");
        assert_eq!(RiskView::calculate_percentage(dec!(25), dec!(200)), "12.5");
    }

    #[test]
    fn format_status_maps_known_and_unknown_statuses() {
        assert_eq!(RiskView::format_status(&Status::Funded), "Entry pending");
        assert_eq!(
            RiskView::format_status(&Status::Submitted),
            "Order submitted"
        );
        assert_eq!(
            RiskView::format_status(&Status::Filled),
            "Target/Stop active"
        );
        assert_eq!(RiskView::format_status(&Status::Canceled), "Unknown status");
    }

    #[test]
    fn display_renders_without_panicking_for_positions_and_empty_state() {
        let positions = vec![OpenPosition {
            trade_id: Uuid::new_v4(),
            symbol: "AAPL".to_string(),
            capital_amount: dec!(1000),
            status: Status::Filled,
            funded_date: Utc::now().naive_utc(),
        }];

        RiskView::display(positions, dec!(1000), dec!(10000));
        RiskView::display(Vec::new(), dec!(0), dec!(0));
    }
}
