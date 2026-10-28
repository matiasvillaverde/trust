//! Broker-account observations and owner-controlled admission contracts.
//! Monetary values serialize as exact decimal strings.
#![allow(missing_docs)]
use chrono::{DateTime, NaiveDate, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use uuid::Uuid;

#[derive(
    Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, schemars::JsonSchema,
)]
#[serde(deny_unknown_fields)]
pub struct BrokerAccountScope {
    pub broker: String,
    pub environment: String,
    pub account_id: String,
}

impl BrokerAccountScope {
    pub fn from_account(account: &crate::Account) -> Result<Self, &'static str> {
        let id = account
            .broker_account_id
            .as_deref()
            .ok_or("broker_account_unbound")?;
        if id.is_empty()
            || id.len() > 128
            || !id
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'-' | b'_'))
        {
            return Err("broker_account_unbound");
        }
        Ok(Self {
            broker: account.broker_kind.to_string(),
            environment: account.environment.to_string(),
            account_id: id.into(),
        })
    }
    pub fn storage_key(&self) -> String {
        format!("{}:{}:{}", self.broker, self.environment, self.account_id)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct BrokerPosition {
    pub instrument_id: String,
    pub symbol: String,
    pub currency: String,
    #[serde(default)]
    pub asset_class: String,
    pub quantity: Decimal,
    pub market_value: Decimal,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct BrokerWorkingOrder {
    pub order_id: String,
    pub client_order_id: Option<String>,
    pub instrument_id: String,
    pub symbol: String,
    pub currency: String,
    #[serde(default)]
    pub asset_class: String,
    pub side: String,
    /// Normalized type: limit, market, stop, stop_limit or unknown.
    #[serde(default)]
    pub order_type: String,
    #[serde(default)]
    pub stop_price: Option<Decimal>,
    /// Confirmed working state; pending submission/cancellation is not protection.
    #[serde(default)]
    pub active: bool,
    pub remaining_quantity: Decimal,
    /// Conservative absolute execution notional. None means unbounded/unknown.
    pub notional: Option<Decimal>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct BrokerAccountSnapshot {
    pub scope: BrokerAccountScope,
    pub currency: String,
    pub observed_at: DateTime<Utc>,
    pub received_at: DateTime<Utc>,
    pub complete: bool,
    pub consistent: bool,
    pub equity: Decimal,
    pub cash: Decimal,
    pub available_funds: Decimal,
    #[serde(default)]
    pub margin: Option<crate::portfolio_risk::BrokerMargin>,
    pub realized_pnl: Decimal,
    pub unrealized_pnl: Decimal,
    pub positions: Vec<BrokerPosition>,
    pub working_orders: Vec<BrokerWorkingOrder>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AccountSafetyPolicy {
    pub currency: String,
    pub max_gross_notional: Decimal,
    pub max_order_notional: Decimal,
    pub max_daily_loss: Decimal,
    pub max_drawdown: Decimal,
    pub max_snapshot_age_seconds: u64,
    /// Absent legacy policies cannot authorize service funding or submission.
    #[serde(default)]
    pub controls: Option<crate::portfolio_risk::PortfolioRiskPolicy>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AgentMandate {
    pub instrument_ids: BTreeSet<Uuid>,
    pub max_order_notional: Decimal,
    pub max_total_notional: Decimal,
    pub max_open_trades: u32,
    pub allow_short: bool,
    pub expires_at: DateTime<Utc>,
    #[serde(default)]
    pub risk_budget: Option<crate::portfolio_risk::AgentRiskBudget>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct EquityBaseline {
    pub day: NaiveDate,
    pub day_start: Decimal,
    pub high_water: Decimal,
    pub last_equity: Decimal,
    pub last_pnl: Decimal,
    pub cash_flow_adjustment: Decimal,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SafetyHalt {
    pub since: DateTime<Utc>,
    pub reasons: BTreeSet<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AccountSafetyState {
    #[serde(default)]
    pub snapshot_fetch_failed: bool,
    pub policy: Option<AccountSafetyPolicy>,
    pub snapshot: Option<BrokerAccountSnapshot>,
    pub baseline: Option<EquityBaseline>,
    pub halt: Option<SafetyHalt>,
    pub mandates: BTreeMap<String, AgentMandate>,
    pub trade_owners: BTreeMap<Uuid, String>,
    /// Ambiguous external operations require explicit, evidenced resolution.
    pub unresolved_operations: BTreeSet<Uuid>,
    pub cash_flow_references: BTreeSet<String>,
    #[serde(default)]
    pub portfolio: crate::portfolio_risk::PortfolioRiskState,
}

impl AccountSafetyState {
    pub fn halt(&mut self, reason: &str, now: DateTime<Utc>) {
        self.halt
            .get_or_insert_with(|| SafetyHalt {
                since: now,
                reasons: BTreeSet::new(),
            })
            .reasons
            .insert(reason.into());
    }
}
