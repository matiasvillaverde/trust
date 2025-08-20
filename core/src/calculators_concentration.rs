//! Concentration analysis module for portfolio risk assessment
//!
//! This module provides functions to analyze portfolio concentration
//! by sector and asset class using precise decimal arithmetic.

use model::trade::{Status, Trade};
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use std::collections::HashMap;

/// Represents a group of trades with concentration metrics
#[derive(Debug, Clone, PartialEq)]
pub struct ConcentrationGroup {
    /// Name of the group (sector or asset class)
    pub name: String,
    /// Number of trades in this group
    pub trade_count: u32,
    /// Total capital deployed to this group
    pub total_capital_deployed: Decimal,
    /// Total realized P&L for this group
    pub realized_pnl: Decimal,
    /// Current capital at risk in open positions
    pub current_open_risk: Decimal,
}

/// Result of concentration analysis
#[derive(Debug, PartialEq)]
pub struct ConcentrationAnalysis {
    /// Groups analyzed (sectors or asset classes)
    pub groups: Vec<ConcentrationGroup>,
    /// Total capital at risk across all groups
    pub total_risk: Decimal,
    /// Groups exceeding risk thresholds
    pub concentration_warnings: Vec<ConcentrationWarning>,
}

/// Warning about concentration risk
#[derive(Debug, PartialEq)]
pub struct ConcentrationWarning {
    /// Group name with high concentration
    pub group_name: String,
    /// Percentage of total risk
    pub risk_percentage: Decimal,
    /// Warning level
    pub level: WarningLevel,
}

/// Severity level of concentration warning
#[derive(Debug, PartialEq)]
pub enum WarningLevel {
    /// Moderate concentration (>50%)
    Moderate,
    /// High concentration (>60%)
    High,
}

/// Calculator for portfolio concentration analysis
#[derive(Debug)]
pub struct ConcentrationCalculator;

impl ConcentrationCalculator {
    /// Analyze concentration by a metadata field (sector or asset_class)
    pub fn analyze_by_metadata(
        trades: &[Trade],
        metadata_field: MetadataField,
    ) -> ConcentrationAnalysis {
        let mut groups_map: HashMap<String, ConcentrationGroup> = HashMap::new();

        for trade in trades {
            // Get the metadata value based on field type
            let group_name = match metadata_field {
                MetadataField::Sector => trade
                    .sector
                    .clone()
                    .unwrap_or_else(|| "Unknown".to_string()),
                MetadataField::AssetClass => trade
                    .asset_class
                    .clone()
                    .unwrap_or_else(|| "Unknown".to_string()),
            };

            // Get or create group
            let group =
                groups_map
                    .entry(group_name.clone())
                    .or_insert_with(|| ConcentrationGroup {
                        name: group_name,
                        trade_count: 0,
                        total_capital_deployed: dec!(0),
                        realized_pnl: dec!(0),
                        current_open_risk: dec!(0),
                    });

            // Update group metrics
            group.trade_count = group.trade_count.saturating_add(1);
            group.total_capital_deployed = group
                .total_capital_deployed
                .checked_add(trade.balance.funding)
                .unwrap_or(group.total_capital_deployed);

            // Add P&L for closed trades
            if matches!(trade.status, Status::ClosedTarget | Status::ClosedStopLoss) {
                group.realized_pnl = group
                    .realized_pnl
                    .checked_add(trade.balance.total_performance)
                    .unwrap_or(group.realized_pnl);
            }

            // Add current risk for open positions
            if matches!(trade.status, Status::Filled | Status::PartiallyFilled) {
                group.current_open_risk = group
                    .current_open_risk
                    .checked_add(trade.balance.capital_in_market)
                    .unwrap_or(group.current_open_risk);
            }
        }

        let groups: Vec<ConcentrationGroup> = groups_map.into_values().collect();
        let total_risk = groups
            .iter()
            .map(|g| g.current_open_risk)
            .fold(dec!(0), |acc, risk| acc.checked_add(risk).unwrap_or(acc));

        let concentration_warnings = Self::calculate_warnings(&groups, total_risk);

        ConcentrationAnalysis {
            groups,
            total_risk,
            concentration_warnings,
        }
    }

    /// Calculate concentration warnings based on risk thresholds
    pub fn calculate_warnings(
        groups: &[ConcentrationGroup],
        total_risk: Decimal,
    ) -> Vec<ConcentrationWarning> {
        let mut warnings = Vec::new();

        // Don't calculate warnings if there's no risk
        if total_risk == dec!(0) {
            return warnings;
        }

        const HUNDRED: Decimal = dec!(100);
        const MODERATE_THRESHOLD: Decimal = dec!(50);
        const HIGH_THRESHOLD: Decimal = dec!(60);

        for group in groups {
            if group.current_open_risk == dec!(0) {
                continue;
            }

            // Calculate percentage of total risk
            let risk_percentage = group
                .current_open_risk
                .checked_mul(HUNDRED)
                .and_then(|v| v.checked_div(total_risk))
                .unwrap_or(dec!(0));

            // Determine warning level
            if risk_percentage > HIGH_THRESHOLD {
                warnings.push(ConcentrationWarning {
                    group_name: group.name.clone(),
                    risk_percentage,
                    level: WarningLevel::High,
                });
            } else if risk_percentage > MODERATE_THRESHOLD {
                warnings.push(ConcentrationWarning {
                    group_name: group.name.clone(),
                    risk_percentage,
                    level: WarningLevel::Moderate,
                });
            }
        }

        warnings
    }

    /// Filter trades to include only open positions
    pub fn filter_open_trades(trades: &[Trade]) -> Vec<Trade> {
        trades
            .iter()
            .filter(|trade| matches!(trade.status, Status::Filled | Status::PartiallyFilled))
            .cloned()
            .collect()
    }
}

/// Metadata field to analyze concentration by
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum MetadataField {
    /// Analyze by sector
    Sector,
    /// Analyze by asset class
    AssetClass,
}

#[cfg(test)]
mod tests {
    use super::*;
    use model::trade::Status;

    fn create_test_trade(
        sector: Option<String>,
        asset_class: Option<String>,
        status: Status,
    ) -> Trade {
        let mut trade = Trade::default();
        trade.sector = sector;
        trade.asset_class = asset_class;
        trade.status = status;
        // Set some basic financial data for testing
        trade.entry.unit_price = dec!(100);
        trade.safety_stop.unit_price = dec!(95);
        trade.target.unit_price = dec!(110);
        trade.safety_stop.quantity = 10;
        trade.balance.funding = dec!(1000);
        trade.balance.capital_in_market =
            if matches!(status, Status::Filled | Status::PartiallyFilled) {
                dec!(1000)
            } else {
                dec!(0)
            };
        trade.balance.total_performance = match status {
            Status::ClosedTarget => dec!(100),   // profit
            Status::ClosedStopLoss => dec!(-50), // loss
            _ => dec!(0),
        };
        trade
    }

    #[test]
    fn test_concentration_group_creation() {
        let group = ConcentrationGroup {
            name: "Technology".to_string(),
            trade_count: 5,
            total_capital_deployed: dec!(10000),
            realized_pnl: dec!(500),
            current_open_risk: dec!(2000),
        };

        assert_eq!(group.name, "Technology");
        assert_eq!(group.trade_count, 5);
        assert_eq!(group.total_capital_deployed, dec!(10000));
        assert_eq!(group.realized_pnl, dec!(500));
        assert_eq!(group.current_open_risk, dec!(2000));
    }

    #[test]
    fn test_analyze_by_sector() {
        let trades = vec![
            create_test_trade(Some("Technology".to_string()), None, Status::ClosedTarget),
            create_test_trade(Some("Technology".to_string()), None, Status::Filled),
            create_test_trade(Some("Healthcare".to_string()), None, Status::ClosedStopLoss),
            create_test_trade(None, None, Status::Filled), // Unknown sector
        ];

        let analysis = ConcentrationCalculator::analyze_by_metadata(&trades, MetadataField::Sector);

        // Should have 3 groups: Technology, Healthcare, Unknown
        assert_eq!(analysis.groups.len(), 3);

        // Find Technology group
        let tech_group = analysis
            .groups
            .iter()
            .find(|g| g.name == "Technology")
            .expect("Technology group should exist");
        assert_eq!(tech_group.trade_count, 2);
    }

    #[test]
    fn test_analyze_by_asset_class() {
        let trades = vec![
            create_test_trade(None, Some("Stocks".to_string()), Status::Filled),
            create_test_trade(None, Some("Stocks".to_string()), Status::ClosedTarget),
            create_test_trade(None, Some("Options".to_string()), Status::PartiallyFilled),
        ];

        let analysis =
            ConcentrationCalculator::analyze_by_metadata(&trades, MetadataField::AssetClass);

        assert_eq!(analysis.groups.len(), 2);

        let stocks_group = analysis
            .groups
            .iter()
            .find(|g| g.name == "Stocks")
            .expect("Stocks group should exist");
        assert_eq!(stocks_group.trade_count, 2);
    }

    #[test]
    fn test_concentration_warnings_moderate() {
        let groups = vec![
            ConcentrationGroup {
                name: "Technology".to_string(),
                trade_count: 10,
                total_capital_deployed: dec!(20000),
                realized_pnl: dec!(1000),
                current_open_risk: dec!(5500), // 55% of total
            },
            ConcentrationGroup {
                name: "Healthcare".to_string(),
                trade_count: 5,
                total_capital_deployed: dec!(10000),
                realized_pnl: dec!(500),
                current_open_risk: dec!(4500), // 45% of total
            },
        ];

        let total_risk = dec!(10000);
        let warnings = ConcentrationCalculator::calculate_warnings(&groups, total_risk);

        assert_eq!(warnings.len(), 1);
        assert_eq!(warnings[0].group_name, "Technology");
        assert_eq!(warnings[0].level, WarningLevel::Moderate);
        assert_eq!(warnings[0].risk_percentage, dec!(55));
    }

    #[test]
    fn test_concentration_warnings_high() {
        let groups = vec![
            ConcentrationGroup {
                name: "Technology".to_string(),
                trade_count: 10,
                total_capital_deployed: dec!(30000),
                realized_pnl: dec!(2000),
                current_open_risk: dec!(6500), // 65% of total
            },
            ConcentrationGroup {
                name: "Healthcare".to_string(),
                trade_count: 5,
                total_capital_deployed: dec!(10000),
                realized_pnl: dec!(500),
                current_open_risk: dec!(3500), // 35% of total
            },
        ];

        let total_risk = dec!(10000);
        let warnings = ConcentrationCalculator::calculate_warnings(&groups, total_risk);

        assert_eq!(warnings.len(), 1);
        assert_eq!(warnings[0].group_name, "Technology");
        assert_eq!(warnings[0].level, WarningLevel::High);
        assert_eq!(warnings[0].risk_percentage, dec!(65));
    }

    #[test]
    fn test_filter_open_trades() {
        let trades = vec![
            create_test_trade(Some("Tech".to_string()), None, Status::Filled),
            create_test_trade(Some("Tech".to_string()), None, Status::ClosedTarget),
            create_test_trade(Some("Health".to_string()), None, Status::PartiallyFilled),
            create_test_trade(Some("Finance".to_string()), None, Status::ClosedStopLoss),
        ];

        let open_trades = ConcentrationCalculator::filter_open_trades(&trades);

        // Should include Filled and PartiallyFilled (active positions)
        assert_eq!(open_trades.len(), 2);
    }

    #[test]
    fn test_empty_trades_analysis() {
        let trades: Vec<Trade> = vec![];
        let analysis = ConcentrationCalculator::analyze_by_metadata(&trades, MetadataField::Sector);

        assert_eq!(analysis.groups.len(), 0);
        assert_eq!(analysis.total_risk, dec!(0));
        assert_eq!(analysis.concentration_warnings.len(), 0);
    }
}
