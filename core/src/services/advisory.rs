#![allow(
    missing_docs,
    clippy::arithmetic_side_effects,
    clippy::indexing_slicing,
    clippy::too_many_lines
)]

use chrono::NaiveDateTime;
use model::Trade;
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use std::collections::HashMap;
use std::error::Error;
use std::fmt;
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq)]
pub struct AdvisoryThresholds {
    pub sector_limit_pct: Decimal,
    pub asset_class_limit_pct: Decimal,
    pub single_position_limit_pct: Decimal,
}

impl Default for AdvisoryThresholds {
    fn default() -> Self {
        Self {
            sector_limit_pct: dec!(30),
            asset_class_limit_pct: dec!(40),
            single_position_limit_pct: dec!(15),
        }
    }
}

impl AdvisoryThresholds {
    /// Validates advisory limit bounds.
    ///
    /// Allowed values are percentages in the inclusive range [0, 100].
    pub fn validate(&self) -> Result<(), AdvisoryThresholdError> {
        let zero = Decimal::ZERO;
        let one_hundred = Decimal::from(100);

        if self.sector_limit_pct < zero {
            return Err(AdvisoryThresholdError::out_of_range(
                "sector_limit_pct",
                self.sector_limit_pct,
            ));
        }
        if self.asset_class_limit_pct < zero {
            return Err(AdvisoryThresholdError::out_of_range(
                "asset_class_limit_pct",
                self.asset_class_limit_pct,
            ));
        }
        if self.single_position_limit_pct < zero {
            return Err(AdvisoryThresholdError::out_of_range(
                "single_position_limit_pct",
                self.single_position_limit_pct,
            ));
        }
        if self.sector_limit_pct > one_hundred {
            return Err(AdvisoryThresholdError::out_of_range(
                "sector_limit_pct",
                self.sector_limit_pct,
            ));
        }
        if self.asset_class_limit_pct > one_hundred {
            return Err(AdvisoryThresholdError::out_of_range(
                "asset_class_limit_pct",
                self.asset_class_limit_pct,
            ));
        }
        if self.single_position_limit_pct > one_hundred {
            return Err(AdvisoryThresholdError::out_of_range(
                "single_position_limit_pct",
                self.single_position_limit_pct,
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AdvisoryThresholdError {
    /// A threshold is outside the allowed [0, 100] percentage range.
    OutOfRange { field: &'static str, value: Decimal },
}

impl AdvisoryThresholdError {
    fn out_of_range(field: &'static str, value: Decimal) -> Self {
        Self::OutOfRange { field, value }
    }
}

impl fmt::Display for AdvisoryThresholdError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::OutOfRange { field, value } => {
                write!(f, "{field} must be between 0 and 100, got {value}")
            }
        }
    }
}

impl Error for AdvisoryThresholdError {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AdvisoryAlertLevel {
    Ok,
    Warning,
    Caution,
    Block,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TradeProposal {
    pub account_id: Uuid,
    pub symbol: String,
    pub sector: Option<String>,
    pub asset_class: Option<String>,
    pub entry_price: Decimal,
    pub quantity: Decimal,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AdvisoryResult {
    pub level: AdvisoryAlertLevel,
    pub warnings: Vec<String>,
    pub recommendations: Vec<String>,
    pub projected_sector_pct: Decimal,
    pub projected_asset_class_pct: Decimal,
    pub projected_single_position_pct: Decimal,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AdvisoryHistoryEntry {
    pub account_id: Uuid,
    pub symbol: String,
    pub level: AdvisoryAlertLevel,
    pub summary: String,
    pub created_at: NaiveDateTime,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PortfolioAdvisoryStatus {
    pub level: AdvisoryAlertLevel,
    pub top_sector_pct: Decimal,
    pub top_asset_class_pct: Decimal,
    pub top_position_pct: Decimal,
    pub warnings: Vec<String>,
}

fn trade_notional(trade: &Trade) -> Decimal {
    trade
        .entry
        .unit_price
        .checked_mul(Decimal::from(trade.entry.quantity))
        .unwrap_or(Decimal::ZERO)
}

fn bounded_pct(numerator: Decimal, denominator: Decimal) -> Decimal {
    if denominator <= Decimal::ZERO {
        return Decimal::ZERO;
    }
    numerator
        .checked_mul(dec!(100))
        .and_then(|n| n.checked_div(denominator))
        .unwrap_or(Decimal::ZERO)
}

fn escalate(current: AdvisoryAlertLevel, next: AdvisoryAlertLevel) -> AdvisoryAlertLevel {
    use AdvisoryAlertLevel::{Block, Caution, Ok, Warning};
    match (current, next) {
        (Block, _) | (_, Block) => Block,
        (Caution, _) | (_, Caution) => Caution,
        (Warning, _) | (_, Warning) => Warning,
        _ => Ok,
    }
}

fn assess_limit(
    value_pct: Decimal,
    limit_pct: Decimal,
    dimension: &str,
    warnings: &mut Vec<String>,
    recs: &mut Vec<String>,
) -> AdvisoryAlertLevel {
    if value_pct > limit_pct.checked_mul(dec!(1.2)).unwrap_or(limit_pct) {
        warnings.push(format!(
            "{dimension} concentration {value_pct}% exceeds hard limit {limit_pct}%"
        ));
        recs.push(format!("Reduce {dimension} exposure below {limit_pct}%"));
        return AdvisoryAlertLevel::Block;
    }
    if value_pct > limit_pct {
        warnings.push(format!(
            "{dimension} concentration {value_pct}% exceeds configured limit {limit_pct}%"
        ));
        recs.push(format!(
            "Consider reducing {dimension} size or diversifying"
        ));
        return AdvisoryAlertLevel::Caution;
    }
    if value_pct > limit_pct.checked_mul(dec!(0.9)).unwrap_or(limit_pct) {
        warnings.push(format!(
            "{dimension} concentration {value_pct}% is near configured limit {limit_pct}%"
        ));
        return AdvisoryAlertLevel::Warning;
    }
    AdvisoryAlertLevel::Ok
}

pub fn analyze_trade_proposal(
    open_trades: &[Trade],
    proposal: &TradeProposal,
    thresholds: &AdvisoryThresholds,
) -> AdvisoryResult {
    let mut sector_exposure: HashMap<String, Decimal> = HashMap::new();
    let mut class_exposure: HashMap<String, Decimal> = HashMap::new();
    let mut symbol_exposure: HashMap<String, Decimal> = HashMap::new();

    let mut total = Decimal::ZERO;
    for trade in open_trades {
        let notional = trade_notional(trade);
        total = total.checked_add(notional).unwrap_or(total);
        let sector = trade
            .sector
            .clone()
            .unwrap_or_else(|| "unknown".to_string());
        let class = trade
            .asset_class
            .clone()
            .unwrap_or_else(|| "unknown".to_string());
        let symbol = trade.trading_vehicle.symbol.clone();
        *sector_exposure.entry(sector).or_insert(Decimal::ZERO) = sector_exposure
            .get(
                &trade
                    .sector
                    .clone()
                    .unwrap_or_else(|| "unknown".to_string()),
            )
            .copied()
            .unwrap_or(Decimal::ZERO)
            .checked_add(notional)
            .unwrap_or(notional);
        *class_exposure.entry(class).or_insert(Decimal::ZERO) = class_exposure
            .get(
                &trade
                    .asset_class
                    .clone()
                    .unwrap_or_else(|| "unknown".to_string()),
            )
            .copied()
            .unwrap_or(Decimal::ZERO)
            .checked_add(notional)
            .unwrap_or(notional);
        *symbol_exposure
            .entry(symbol.clone())
            .or_insert(Decimal::ZERO) = symbol_exposure
            .get(&symbol)
            .copied()
            .unwrap_or(Decimal::ZERO)
            .checked_add(notional)
            .unwrap_or(notional);
    }

    let proposal_notional = proposal
        .entry_price
        .checked_mul(proposal.quantity)
        .unwrap_or(Decimal::ZERO);
    total = total.checked_add(proposal_notional).unwrap_or(total);

    let sector_key = proposal
        .sector
        .clone()
        .unwrap_or_else(|| "unknown".to_string());
    let class_key = proposal
        .asset_class
        .clone()
        .unwrap_or_else(|| "unknown".to_string());
    let symbol_key = proposal.symbol.clone();

    let projected_sector_notional = sector_exposure
        .get(&sector_key)
        .copied()
        .unwrap_or(Decimal::ZERO)
        .checked_add(proposal_notional)
        .unwrap_or(proposal_notional);
    let projected_class_notional = class_exposure
        .get(&class_key)
        .copied()
        .unwrap_or(Decimal::ZERO)
        .checked_add(proposal_notional)
        .unwrap_or(proposal_notional);
    let projected_symbol_notional = symbol_exposure
        .get(&symbol_key)
        .copied()
        .unwrap_or(Decimal::ZERO)
        .checked_add(proposal_notional)
        .unwrap_or(proposal_notional);

    let sector_pct = bounded_pct(projected_sector_notional, total);
    let class_pct = bounded_pct(projected_class_notional, total);
    let single_pct = bounded_pct(projected_symbol_notional, total);

    let mut warnings = Vec::new();
    let mut recommendations = Vec::new();
    let mut level = AdvisoryAlertLevel::Ok;
    level = escalate(
        level,
        assess_limit(
            sector_pct,
            thresholds.sector_limit_pct,
            "sector",
            &mut warnings,
            &mut recommendations,
        ),
    );
    level = escalate(
        level,
        assess_limit(
            class_pct,
            thresholds.asset_class_limit_pct,
            "asset_class",
            &mut warnings,
            &mut recommendations,
        ),
    );
    level = escalate(
        level,
        assess_limit(
            single_pct,
            thresholds.single_position_limit_pct,
            "single_position",
            &mut warnings,
            &mut recommendations,
        ),
    );

    recommendations.sort();
    recommendations.dedup();
    AdvisoryResult {
        level,
        warnings,
        recommendations,
        projected_sector_pct: sector_pct,
        projected_asset_class_pct: class_pct,
        projected_single_position_pct: single_pct,
    }
}

pub fn portfolio_status(
    open_trades: &[Trade],
    thresholds: &AdvisoryThresholds,
) -> PortfolioAdvisoryStatus {
    let mut total = Decimal::ZERO;
    let mut sector: HashMap<String, Decimal> = HashMap::new();
    let mut class: HashMap<String, Decimal> = HashMap::new();
    let mut symbol: HashMap<String, Decimal> = HashMap::new();

    for trade in open_trades {
        let notional = trade_notional(trade);
        total = total.checked_add(notional).unwrap_or(total);
        let s = trade
            .sector
            .clone()
            .unwrap_or_else(|| "unknown".to_string());
        let c = trade
            .asset_class
            .clone()
            .unwrap_or_else(|| "unknown".to_string());
        let y = trade.trading_vehicle.symbol.clone();
        *sector.entry(s).or_insert(Decimal::ZERO) = sector
            .get(
                &trade
                    .sector
                    .clone()
                    .unwrap_or_else(|| "unknown".to_string()),
            )
            .copied()
            .unwrap_or(Decimal::ZERO)
            .checked_add(notional)
            .unwrap_or(notional);
        *class.entry(c).or_insert(Decimal::ZERO) = class
            .get(
                &trade
                    .asset_class
                    .clone()
                    .unwrap_or_else(|| "unknown".to_string()),
            )
            .copied()
            .unwrap_or(Decimal::ZERO)
            .checked_add(notional)
            .unwrap_or(notional);
        *symbol.entry(y.clone()).or_insert(Decimal::ZERO) = symbol
            .get(&y)
            .copied()
            .unwrap_or(Decimal::ZERO)
            .checked_add(notional)
            .unwrap_or(notional);
    }

    let top_sector = sector
        .values()
        .copied()
        .max()
        .map(|n| bounded_pct(n, total))
        .unwrap_or(Decimal::ZERO);
    let top_class = class
        .values()
        .copied()
        .max()
        .map(|n| bounded_pct(n, total))
        .unwrap_or(Decimal::ZERO);
    let top_symbol = symbol
        .values()
        .copied()
        .max()
        .map(|n| bounded_pct(n, total))
        .unwrap_or(Decimal::ZERO);

    let mut warnings = Vec::new();
    let mut level = AdvisoryAlertLevel::Ok;
    level = escalate(
        level,
        assess_limit(
            top_sector,
            thresholds.sector_limit_pct,
            "sector",
            &mut warnings,
            &mut Vec::new(),
        ),
    );
    level = escalate(
        level,
        assess_limit(
            top_class,
            thresholds.asset_class_limit_pct,
            "asset_class",
            &mut warnings,
            &mut Vec::new(),
        ),
    );
    level = escalate(
        level,
        assess_limit(
            top_symbol,
            thresholds.single_position_limit_pct,
            "single_position",
            &mut warnings,
            &mut Vec::new(),
        ),
    );

    PortfolioAdvisoryStatus {
        level,
        top_sector_pct: top_sector,
        top_asset_class_pct: top_class,
        top_position_pct: top_symbol,
        warnings,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use model::Trade;
    use rust_decimal_macros::dec;
    use uuid::Uuid;

    fn open_trade(
        symbol: &str,
        sector: Option<&str>,
        asset_class: Option<&str>,
        notional: Decimal,
    ) -> Trade {
        let mut trade = Trade::default();
        trade.trading_vehicle.symbol = symbol.to_string();
        trade.sector = sector.map(str::to_string);
        trade.asset_class = asset_class.map(str::to_string);
        trade.entry.unit_price = notional;
        trade.entry.quantity = 1;
        trade
    }

    fn proposal(
        symbol: &str,
        sector: Option<&str>,
        asset_class: Option<&str>,
        notional: Decimal,
    ) -> TradeProposal {
        TradeProposal {
            account_id: Uuid::new_v4(),
            symbol: symbol.to_string(),
            sector: sector.map(str::to_string),
            asset_class: asset_class.map(str::to_string),
            entry_price: notional,
            quantity: Decimal::ONE,
        }
    }

    fn thresholds(
        sector_limit_pct: Decimal,
        asset_class_limit_pct: Decimal,
        single_position_limit_pct: Decimal,
    ) -> AdvisoryThresholds {
        AdvisoryThresholds {
            sector_limit_pct,
            asset_class_limit_pct,
            single_position_limit_pct,
        }
    }

    #[test]
    fn advisory_levels_escalate_to_caution() {
        let mut open = Trade::default();
        open.trading_vehicle.symbol = "AAPL".to_string();
        open.sector = Some("technology".to_string());
        open.asset_class = Some("stocks".to_string());
        open.entry.unit_price = dec!(100);
        open.entry.quantity = 100;

        let proposal = TradeProposal {
            account_id: Uuid::new_v4(),
            symbol: "MSFT".to_string(),
            sector: Some("technology".to_string()),
            asset_class: Some("stocks".to_string()),
            entry_price: dec!(100),
            quantity: dec!(100),
        };
        let out = analyze_trade_proposal(&[open], &proposal, &AdvisoryThresholds::default());
        assert!(matches!(out.level, AdvisoryAlertLevel::Block));
        assert!(!out.warnings.is_empty());
    }

    #[test]
    fn proposal_analysis_distinguishes_ok_warning_caution_and_block() {
        let unrelated = open_trade("BND", Some("fixed_income"), Some("bonds"), dec!(54));
        let warning = analyze_trade_proposal(
            std::slice::from_ref(&unrelated),
            &proposal("MSFT", Some("technology"), Some("stocks"), dec!(46)),
            &thresholds(dec!(50), dec!(100), dec!(100)),
        );
        assert_eq!(warning.level, AdvisoryAlertLevel::Warning);
        assert_eq!(warning.projected_sector_pct, dec!(46));
        assert!(warning
            .warnings
            .iter()
            .any(|warning| warning.contains("near configured limit")));
        assert!(warning.recommendations.is_empty());

        let caution = analyze_trade_proposal(
            &[open_trade(
                "BND",
                Some("fixed_income"),
                Some("bonds"),
                dec!(45),
            )],
            &proposal("MSFT", Some("technology"), Some("stocks"), dec!(55)),
            &thresholds(dec!(50), dec!(100), dec!(100)),
        );
        assert_eq!(caution.level, AdvisoryAlertLevel::Caution);
        assert_eq!(caution.projected_sector_pct, dec!(55));
        assert!(caution
            .recommendations
            .iter()
            .any(|recommendation| recommendation.contains("diversifying")));

        let block = analyze_trade_proposal(
            &[open_trade(
                "BND",
                Some("fixed_income"),
                Some("bonds"),
                dec!(39),
            )],
            &proposal("MSFT", Some("technology"), Some("stocks"), dec!(61)),
            &thresholds(dec!(50), dec!(100), dec!(100)),
        );
        assert_eq!(block.level, AdvisoryAlertLevel::Block);
        assert_eq!(block.projected_sector_pct, dec!(61));
        assert!(block
            .warnings
            .iter()
            .any(|warning| warning.contains("hard limit")));

        let ok = analyze_trade_proposal(
            &[open_trade(
                "BND",
                Some("fixed_income"),
                Some("bonds"),
                dec!(91),
            )],
            &proposal("MSFT", Some("technology"), Some("stocks"), dec!(9)),
            &thresholds(dec!(50), dec!(50), dec!(50)),
        );
        assert_eq!(ok.level, AdvisoryAlertLevel::Ok);
        assert!(ok.warnings.is_empty());
        assert!(ok.recommendations.is_empty());
    }

    #[test]
    fn proposal_analysis_groups_missing_metadata_under_unknown_bucket() {
        let open = open_trade("AAPL", None, None, dec!(50));
        let output = analyze_trade_proposal(
            &[open],
            &proposal("MSFT", None, None, dec!(50)),
            &thresholds(dec!(80), dec!(80), dec!(100)),
        );

        assert_eq!(output.level, AdvisoryAlertLevel::Block);
        assert_eq!(output.projected_sector_pct, dec!(100));
        assert_eq!(output.projected_asset_class_pct, dec!(100));
        assert_eq!(output.projected_single_position_pct, dec!(50));
        assert_eq!(output.warnings.len(), 2);
    }

    #[test]
    fn portfolio_status_handles_empty_and_concentrated_portfolios() {
        let empty = portfolio_status(&[], &AdvisoryThresholds::default());
        assert_eq!(empty.level, AdvisoryAlertLevel::Ok);
        assert_eq!(empty.top_sector_pct, Decimal::ZERO);
        assert_eq!(empty.top_asset_class_pct, Decimal::ZERO);
        assert_eq!(empty.top_position_pct, Decimal::ZERO);
        assert!(empty.warnings.is_empty());

        let concentrated = portfolio_status(
            &[
                open_trade("AAPL", Some("technology"), Some("stocks"), dec!(60)),
                open_trade("BND", Some("fixed_income"), Some("bonds"), dec!(40)),
            ],
            &thresholds(dec!(50), dec!(100), dec!(100)),
        );
        assert_eq!(concentrated.level, AdvisoryAlertLevel::Caution);
        assert_eq!(concentrated.top_sector_pct, dec!(60));
        assert_eq!(concentrated.top_asset_class_pct, dec!(60));
        assert_eq!(concentrated.top_position_pct, dec!(60));
        assert_eq!(concentrated.warnings.len(), 1);
    }

    #[test]
    fn portfolio_status_uses_unknown_bucket_for_missing_metadata() {
        let status = portfolio_status(
            &[
                open_trade("AAPL", None, None, dec!(40)),
                open_trade("MSFT", None, None, dec!(60)),
            ],
            &thresholds(dec!(80), dec!(80), dec!(100)),
        );

        assert_eq!(status.level, AdvisoryAlertLevel::Block);
        assert_eq!(status.top_sector_pct, dec!(100));
        assert_eq!(status.top_asset_class_pct, dec!(100));
        assert_eq!(status.top_position_pct, dec!(60));
        assert_eq!(status.warnings.len(), 2);
    }

    #[test]
    fn advisory_thresholds_validation_allows_valid_bounds() {
        let thresholds = AdvisoryThresholds {
            sector_limit_pct: dec!(0),
            asset_class_limit_pct: dec!(50),
            single_position_limit_pct: dec!(100),
        };
        assert!(thresholds.validate().is_ok());
    }

    #[test]
    fn advisory_thresholds_validation_rejects_negative() {
        let thresholds = AdvisoryThresholds {
            sector_limit_pct: dec!(-1),
            asset_class_limit_pct: dec!(50),
            single_position_limit_pct: dec!(20),
        };
        let error = thresholds.validate();
        assert!(error.is_err());
        assert_eq!(
            error.unwrap_err().to_string(),
            "sector_limit_pct must be between 0 and 100, got -1"
        );
    }

    #[test]
    fn advisory_thresholds_validation_rejects_over_100() {
        let thresholds = AdvisoryThresholds {
            sector_limit_pct: dec!(20),
            asset_class_limit_pct: dec!(101),
            single_position_limit_pct: dec!(20),
        };
        let error = thresholds.validate();
        assert!(error.is_err());
        assert_eq!(
            error.unwrap_err().to_string(),
            "asset_class_limit_pct must be between 0 and 100, got 101"
        );
    }

    #[test]
    fn advisory_thresholds_validation_reports_all_invalid_fields() {
        let cases = [
            (
                thresholds(dec!(-1), dec!(50), dec!(20)),
                "sector_limit_pct must be between 0 and 100, got -1",
            ),
            (
                thresholds(dec!(20), dec!(-1), dec!(20)),
                "asset_class_limit_pct must be between 0 and 100, got -1",
            ),
            (
                thresholds(dec!(20), dec!(50), dec!(-1)),
                "single_position_limit_pct must be between 0 and 100, got -1",
            ),
            (
                thresholds(dec!(101), dec!(50), dec!(20)),
                "sector_limit_pct must be between 0 and 100, got 101",
            ),
            (
                thresholds(dec!(20), dec!(101), dec!(20)),
                "asset_class_limit_pct must be between 0 and 100, got 101",
            ),
            (
                thresholds(dec!(20), dec!(50), dec!(101)),
                "single_position_limit_pct must be between 0 and 100, got 101",
            ),
        ];

        for (thresholds, message) in cases {
            assert_eq!(thresholds.validate().unwrap_err().to_string(), message);
        }
    }
}
