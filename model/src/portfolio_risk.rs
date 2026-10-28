//! Owner policy and broker evidence for prospective portfolio risk.
#![allow(missing_docs)]
use crate::{account_safety::BrokerAccountScope, MarketBar};
use chrono::{DateTime, NaiveDate, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PortfolioRiskPolicy {
    pub max_portfolio_heat_pct: Decimal,
    pub max_trade_heat_pct: Decimal,
    pub max_stress_loss_pct: Decimal,
    pub min_margin_cushion_pct: Decimal,
    pub maintenance_margin_stress_pct: Decimal,
    pub max_instrument_pct: Decimal,
    pub max_sector_pct: Decimal,
    pub max_cluster_pct: Decimal,
    pub max_quote_age_seconds: u64,
    pub max_history_age_seconds: u64,
    pub volatility_lookback_bars: u32,
    pub atr_multiplier: Decimal,
    pub max_spread_bps: Decimal,
    pub max_entry_deviation_bps: Decimal,
    pub max_volume_participation_pct: Decimal,
    pub max_book_participation_pct: Decimal,
    pub slippage_bps: Decimal,
    pub fee_per_unit: Decimal,
    pub minimum_fee: Decimal,
    pub drawdown_steps: Vec<DrawdownStep>,
    pub recovery_delay_seconds: u64,
    pub recovery_step: Decimal,
    /// Broker-native IDs, never agent-supplied trade sector/thesis metadata.
    pub instruments: BTreeMap<String, InstrumentRiskPolicy>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DrawdownStep {
    pub drawdown_pct: Decimal,
    pub risk_multiplier: Decimal,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct InstrumentRiskPolicy {
    pub sector: String,
    pub correlation_group: String,
    /// Simultaneous adverse move for both long and short stress scenarios.
    pub gap_stress_pct: Decimal,
    /// Minimum modeled loss even when a stop is very close to the mark.
    pub minimum_risk_pct: Decimal,
    /// Explicit UTC entry windows; exclude overnight/weekend exposure as required.
    pub entry_windows: Vec<RiskWindow>,
    /// Owner-maintained earnings/event exclusions. No calendar is inferred.
    pub blackouts: Vec<RiskWindow>,
    pub valid_until: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RiskWindow {
    pub starts_at: DateTime<Utc>,
    pub ends_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AgentRiskBudget {
    pub max_daily_loss: Decimal,
    /// Conservative round-trip notional charged when capital is committed.
    pub max_daily_turnover: Decimal,
    pub max_daily_entries: u32,
    pub reentry_cooldown_seconds: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct BrokerMargin {
    pub observed_at: DateTime<Utc>,
    pub maintenance_margin: Decimal,
    pub excess_liquidity: Decimal,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RiskMarketData {
    pub scope: BrokerAccountScope,
    pub instrument_id: String,
    pub observed_at: DateTime<Utc>,
    pub received_at: DateTime<Utc>,
    pub real_time: bool,
    pub bid: Decimal,
    pub ask: Decimal,
    pub bid_size: Decimal,
    pub ask_size: Decimal,
    /// Completed one-day bars, including the prior close needed for true range.
    pub daily_bars: Vec<MarketBar>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RiskReservation {
    pub request_id: Uuid,
    pub subject: String,
    pub instrument_id: String,
    pub created_at: DateTime<Utc>,
    pub planned_loss: Decimal,
    pub stress_loss: Decimal,
    pub notional: Decimal,
    pub minimum_unit_risk: Decimal,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct EntryCharge {
    pub request_id: Uuid,
    pub trade_id: Uuid,
    pub subject: String,
    pub instrument_id: String,
    pub at: DateTime<Utc>,
    pub turnover: Decimal,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AgentLossState {
    pub day: NaiveDate,
    pub day_start_pnl: Decimal,
    pub last_pnl: Decimal,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TradeTurnover {
    pub day: NaiveDate,
    pub cumulative: Decimal,
    pub today: Decimal,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DrawdownThrottle {
    pub multiplier: Decimal,
    pub changed_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct PortfolioRiskState {
    pub reservations: BTreeMap<Uuid, RiskReservation>,
    pub entry_charges: Vec<EntryCharge>,
    pub agent_losses: BTreeMap<String, AgentLossState>,
    pub throttle: Option<DrawdownThrottle>,
    pub turnover: BTreeMap<Uuid, TradeTurnover>,
    pub market: BTreeMap<String, RiskMarketData>,
}
