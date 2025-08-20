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
