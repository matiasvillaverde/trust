//! Independent broker reports and exact, read-only reconciliation evidence.
#![allow(missing_docs)]
use chrono::{DateTime, NaiveDate, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use uuid::Uuid;

pub const MAX_STATEMENT_BYTES: usize = 262_144;
pub const MAX_LEDGER_ROWS: usize = 20_000;

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct BrokerStatement {
    pub broker_account_id: String,
    pub from_date: NaiveDate,
    pub to_date: NaiveDate,
    pub timezone: String,
    pub start: DateTime<Utc>,
    pub end_exclusive: DateTime<Utc>,
    pub generated_at: DateTime<Utc>,
    pub cash: BTreeMap<String, StatementCash>,
    pub executions: Vec<StatementExecution>,
    pub positions: Vec<StatementPosition>,
    pub cash_flows: Vec<StatementCashFlow>,
    /// Unsupported content is retained as a blocker, never silently discarded.
    pub blockers: BTreeSet<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct StatementCash {
    pub opening: Decimal,
    pub closing: Decimal,
    /// Positive commission cost.
    pub commissions: Decimal,
    pub net_deposits: Decimal,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct StatementExecution {
    pub execution_id: String,
    pub instrument_id: String,
    pub symbol: String,
    pub currency: String,
    pub buy: bool,
    pub quantity: Decimal,
    pub price: Decimal,
    pub occurred_at: DateTime<Utc>,
    pub commission: Decimal,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct StatementPosition {
    pub instrument_id: String,
    pub currency: String,
    pub quantity: Decimal,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct StatementCashFlow {
    pub reference: String,
    pub currency: String,
    pub amount: Decimal,
    pub occurred_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct RecordedExecution {
    pub account_id: Uuid,
    pub trade_id: Uuid,
    pub execution: StatementExecution,
}

#[derive(Debug, Clone)]
pub struct RecordedFee {
    pub account_id: Uuid,
    pub trade_id: Option<Uuid>,
    pub activity_id: String,
    pub amount: Decimal,
    pub occurred_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct RecordedMovement {
    pub account_id: Uuid,
    pub trade_id: Option<Uuid>,
    pub category: String,
    pub currency: String,
    pub amount: Decimal,
    pub occurred_at: DateTime<Utc>,
}

#[derive(Debug, Default)]
pub struct StatementLedger {
    pub trades: Vec<RecordedTrade>,
    pub executions: Vec<RecordedExecution>,
    pub fees: Vec<RecordedFee>,
    pub movements: Vec<RecordedMovement>,
}

#[derive(Debug, Clone)]
pub struct RecordedTrade {
    pub account_id: Uuid,
    pub trade_id: Uuid,
    pub entry_buy: bool,
    pub entry_filled: Decimal,
    pub exit_filled: Decimal,
}

/// A bounded, consistent read of native broker executions and actual receipts.
/// Derived order-snapshot deltas are excluded; core requires native evidence to
/// cover recorded filled quantities. Available-cash reservations are not cash.
pub trait ReadStatementDB {
    fn read_statement_ledger(
        &mut self,
        accounts: &BTreeSet<Uuid>,
    ) -> Result<StatementLedger, Box<dyn std::error::Error>>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ReconciliationStatus {
    Matched,
    Discrepancies,
    Inconclusive,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CashComparison {
    pub currency: String,
    pub broker_opening: Decimal,
    pub trust_opening: Decimal,
    pub broker_closing: Decimal,
    pub trust_closing: Decimal,
    pub broker_net_deposits: Decimal,
    pub trust_net_deposits: Decimal,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct StatementIssue {
    pub code: String,
    pub reference: Option<String>,
    pub currency: Option<String>,
    pub broker_amount: Option<Decimal>,
    pub trust_amount: Option<Decimal>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct StatementReconciliation {
    pub status: ReconciliationStatus,
    pub checked_at: DateTime<Utc>,
    pub execution_count: usize,
    pub position_count: usize,
    pub issue_count: usize,
    pub issues: Vec<StatementIssue>,
    pub cash: Vec<CashComparison>,
    pub cash_tolerance: Decimal,
    pub permits_trading: bool,
}

impl StatementReconciliation {
    pub fn inconclusive(code: &str) -> Self {
        Self {
            status: ReconciliationStatus::Inconclusive,
            checked_at: Utc::now(),
            execution_count: 0,
            position_count: 0,
            issue_count: 1,
            issues: vec![StatementIssue {
                code: code.into(),
                reference: None,
                currency: None,
                broker_amount: None,
                trust_amount: None,
            }],
            cash: vec![],
            cash_tolerance: Decimal::new(1, 2),
            permits_trading: false,
        }
    }
}
