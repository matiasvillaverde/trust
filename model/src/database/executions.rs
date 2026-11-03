use crate::Execution;
use std::error::Error;
use uuid::Uuid;

/// Trait for reading execution data from the database.
pub trait ReadExecutionDB {
    /// Retrieve all executions for a trade.
    fn all_trade_executions(&mut self, trade_id: Uuid) -> Result<Vec<Execution>, Box<dyn Error>>;

    /// Retrieve all executions for an order.
    fn all_order_executions(&mut self, order_id: Uuid) -> Result<Vec<Execution>, Box<dyn Error>>;

    /// Retrieve the latest execution timestamp for a trade (if any).
    fn latest_trade_execution_at(
        &mut self,
        trade_id: Uuid,
    ) -> Result<Option<chrono::NaiveDateTime>, Box<dyn Error>>;
}

/// Trait for writing execution data to the database.
pub trait WriteExecutionDB {
    /// Persist an account-scoped fee identity, optionally allocating it to a trade.
    /// Returns true only for its first allocation. Unattributable fees remain unallocated.
    /// Implementations must reject conflicting reuse of an identity.
    fn record_fee_activity(
        &mut self,
        _fee: &crate::FeeActivity,
        _trade_id: Option<Uuid>,
    ) -> Result<bool, Box<dyn Error>> {
        Err("Fee activity persistence is unsupported".into())
    }

    /// Insert an execution if not already present (idempotent).
    ///
    /// Dedupe is expected to be enforced by `(broker, account_id, broker_execution_id)`.
    fn upsert_execution(&mut self, execution: &Execution) -> Result<Execution, Box<dyn Error>>;
}
