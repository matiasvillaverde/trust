//! A fixed set of financial stages. Caller-provided strings never become labels.
use super::{increment, Histogram};
use std::{
    collections::BTreeMap,
    sync::atomic::{AtomicU64, Ordering},
    time::Duration,
};

const NAMES: [&str; 16] = [
    "risk.available_cash",
    "risk.monthly_reference",
    "risk.month_opening",
    "risk.carried_trades",
    "risk.quantity",
    "risk.monthly_percentage",
    "risk.protected_capital",
    "db.cash_movements",
    "db.cash_projection.append",
    "db.all_transactions",
    "db.account_transactions",
    "db.opening_transactions",
    "db.monthly_reference_inputs",
    "db.ledger.load",
    "db.ledger.decode_transactions",
    "db.ledger.decode_cash",
];
pub(crate) fn known(name: &str) -> bool {
    NAMES.contains(&name)
}
#[derive(Default)]
struct Stage {
    duration: Histogram,
    rows: AtomicU64,
}
pub(crate) struct FinancialMetrics(BTreeMap<&'static str, Stage>);
impl Default for FinancialMetrics {
    fn default() -> Self {
        Self(
            NAMES
                .into_iter()
                .map(|name| (name, Stage::default()))
                .collect(),
        )
    }
}
impl FinancialMetrics {
    pub(crate) fn observe(&self, name: &str, duration: Duration, rows: u64) {
        if let Some(stage) = self.0.get(name) {
            stage.duration.observe(duration);
            increment(&stage.rows, rows);
        }
    }
    pub(super) fn render(&self, text: &mut String) {
        text.push_str("# TYPE trust_financial_stage_duration_seconds histogram\n# TYPE trust_financial_rows_read_total counter\n");
        for (name, stage) in &self.0 {
            let labels = format!("stage=\"{name}\"");
            stage
                .duration
                .seconds("trust_financial_stage_duration_seconds", &labels, text);
            text.push_str(&format!(
                "trust_financial_rows_read_total{{{labels}}} {}\n",
                stage.rows.load(Ordering::Relaxed)
            ));
        }
    }
}
