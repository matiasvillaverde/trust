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
}
