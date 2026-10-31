use core::calculators_concentration::{ConcentrationAnalysis, ConcentrationGroup, WarningLevel};
use rust_decimal::Decimal;
use rust_decimal_macros::dec;

pub struct ConcentrationView;

impl ConcentrationView {
    pub fn display(
        sector_analysis: ConcentrationAnalysis,
        asset_class_analysis: ConcentrationAnalysis,
        open_only: bool,
    ) {
        println!("\nPortfolio Concentration Analysis");
        println!("=================================");

        if open_only {
            println!("(Showing open positions only)\n");
        } else {
            println!("(Showing all trades)\n");
        }

        // Display sector concentration
        if !sector_analysis.groups.is_empty() {
            println!("By Sector:");
            Self::display_groups(&sector_analysis.groups, sector_analysis.total_risk);

            if !sector_analysis.concentration_warnings.is_empty() {
                println!();
                Self::display_warnings(&sector_analysis.concentration_warnings);
            }
        }

        // Display asset class concentration
        if !asset_class_analysis.groups.is_empty() {
            println!("\nBy Asset Class:");
            Self::display_groups(
                &asset_class_analysis.groups,
                asset_class_analysis.total_risk,
            );

            if !asset_class_analysis.concentration_warnings.is_empty() {
                println!();
                Self::display_warnings(&asset_class_analysis.concentration_warnings);
            }
        }

        // Display total risk summary
        if sector_analysis.total_risk > dec!(0) {
            println!(
                "\nTotal Capital at Risk: {}",
                crate::zen::currency_share(sector_analysis.total_risk, sector_analysis.total_risk)
            );
        }

        println!();
    }

    fn display_groups(groups: &[ConcentrationGroup], total_risk: Decimal) {
        // Sort groups by current open risk (descending)
        let mut sorted_groups = groups.to_vec();
        sorted_groups.sort_by_key(|group| std::cmp::Reverse(group.current_open_risk));

        for group in sorted_groups {
            let pnl_display = if crate::zen::is_enabled() {
                crate::zen::signed_percentage_of(group.realized_pnl, group.total_capital_deployed)
            } else if group.realized_pnl >= dec!(0) {
                format!("+${:.2}", group.realized_pnl)
            } else {
                format!("-${:.2}", group.realized_pnl.abs())
            };

            // Calculate P&L percentage if there's deployed capital
            let pnl_percentage = if group.total_capital_deployed > dec!(0) {
                group
                    .realized_pnl
                    .checked_mul(dec!(100))
                    .and_then(|v| v.checked_div(group.total_capital_deployed))
                    .unwrap_or(dec!(0))
            } else {
                dec!(0)
            };

            let pnl_percentage_display = if pnl_percentage >= dec!(0) {
                format!("+{pnl_percentage:.1}%")
            } else {
                format!("{pnl_percentage:.1}%")
            };
            let deployed_display = if crate::zen::is_enabled() {
                crate::zen::amount_share(group.total_capital_deployed, group.total_capital_deployed)
            } else {
                format!("${:.2}", group.total_capital_deployed)
            };

            println!(
                "{}: {} trades, {} deployed, {} P&L ({})",
                group.name,
                group.trade_count,
                deployed_display,
                pnl_display,
                pnl_percentage_display
            );

            if group.current_open_risk > dec!(0) {
                println!(
                    "  └─ Current open risk: {}",
                    crate::zen::currency_share(group.current_open_risk, total_risk)
                );
            }
        }
    }

    fn display_warnings(warnings: &[core::calculators_concentration::ConcentrationWarning]) {
        for warning in warnings {
            let icon = Self::warning_icon(&warning.level);

            println!(
                "{} Risk Concentration Alert: {:.1}% of open risk in {} sector",
                icon, warning.risk_percentage, warning.group_name
            );
        }
    }

    fn warning_icon(level: &WarningLevel) -> &'static str {
        match level {
            WarningLevel::High => "🔴",
            WarningLevel::Moderate => "⚠️",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::ConcentrationView;
    use core::calculators_concentration::WarningLevel;

    #[test]
    fn warning_icon_maps_all_levels() {
        assert_eq!(ConcentrationView::warning_icon(&WarningLevel::High), "🔴");
        assert_eq!(
            ConcentrationView::warning_icon(&WarningLevel::Moderate),
            "⚠️"
        );
    }
}
