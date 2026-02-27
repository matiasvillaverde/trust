use core::calculators_concentration::{ConcentrationAnalysis, ConcentrationGroup, WarningLevel};
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
            Self::display_groups(&sector_analysis.groups);

            if !sector_analysis.concentration_warnings.is_empty() {
                println!();
                Self::display_warnings(&sector_analysis.concentration_warnings);
            }
        }

        // Display asset class concentration
        if !asset_class_analysis.groups.is_empty() {
            println!("\nBy Asset Class:");
            Self::display_groups(&asset_class_analysis.groups);

            if !asset_class_analysis.concentration_warnings.is_empty() {
                println!();
                Self::display_warnings(&asset_class_analysis.concentration_warnings);
            }
        }

        // Display total risk summary
        if sector_analysis.total_risk > dec!(0) {
            println!(
                "\nTotal Capital at Risk: ${:.2}",
                sector_analysis.total_risk
            );
        }

        println!();
    }

    fn display_groups(groups: &[ConcentrationGroup]) {
        // Sort groups by current open risk (descending)
        let mut sorted_groups = groups.to_vec();
        sorted_groups.sort_by(|a, b| b.current_open_risk.cmp(&a.current_open_risk));

        for group in sorted_groups {
            let pnl_display = if group.realized_pnl >= dec!(0) {
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

            println!(
                "{}: {} trades, ${:.2} deployed, {} P&L ({})",
                group.name,
                group.trade_count,
                group.total_capital_deployed,
                pnl_display,
                pnl_percentage_display
            );

            if group.current_open_risk > dec!(0) {
                println!("  └─ Current open risk: ${:.2}", group.current_open_risk);
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
    use core::calculators_concentration::{
        ConcentrationAnalysis, ConcentrationGroup, ConcentrationWarning, WarningLevel,
    };
    use rust_decimal_macros::dec;

    fn group(
        name: &str,
        risk: rust_decimal::Decimal,
        pnl: rust_decimal::Decimal,
    ) -> ConcentrationGroup {
        ConcentrationGroup {
            name: name.to_string(),
            trade_count: 2,
            total_capital_deployed: dec!(1000),
            realized_pnl: pnl,
            current_open_risk: risk,
        }
    }

    #[test]
    fn warning_icon_maps_all_levels() {
        assert_eq!(ConcentrationView::warning_icon(&WarningLevel::High), "🔴");
        assert_eq!(
            ConcentrationView::warning_icon(&WarningLevel::Moderate),
            "⚠️"
        );
    }

    #[test]
    fn display_groups_and_warnings_handle_positive_negative_and_zero_deployed() {
        let groups = vec![
            group("Tech", dec!(500), dec!(100)),
            group("Energy", dec!(0), dec!(-30)),
            ConcentrationGroup {
                total_capital_deployed: dec!(0),
                ..group("Cash", dec!(10), dec!(10))
            },
        ];
        let warnings = vec![
            ConcentrationWarning {
                group_name: "Tech".to_string(),
                risk_percentage: dec!(70),
                level: WarningLevel::High,
            },
            ConcentrationWarning {
                group_name: "Energy".to_string(),
                risk_percentage: dec!(55),
                level: WarningLevel::Moderate,
            },
        ];

        ConcentrationView::display_groups(&groups);
        ConcentrationView::display_warnings(&warnings);
    }

    #[test]
    fn display_handles_open_only_and_all_modes() {
        let sector_analysis = ConcentrationAnalysis {
            groups: vec![group("Tech", dec!(250), dec!(50))],
            total_risk: dec!(250),
            concentration_warnings: vec![],
        };
        let asset_analysis = ConcentrationAnalysis {
            groups: vec![],
            total_risk: dec!(0),
            concentration_warnings: vec![],
        };

        ConcentrationView::display(sector_analysis, asset_analysis, true);

        let empty = ConcentrationAnalysis {
            groups: vec![],
            total_risk: dec!(0),
            concentration_warnings: vec![],
        };
        ConcentrationView::display(
            empty,
            ConcentrationAnalysis {
                groups: vec![],
                total_risk: dec!(0),
                concentration_warnings: vec![],
            },
            false,
        );
    }
}
