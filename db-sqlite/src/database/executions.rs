use super::{lock_connection_or_exit, SqliteDatabase};
use crate::workers::WorkerExecution;
use model::{Execution, ReadExecutionDB, WriteExecutionDB};
use std::error::Error;
use uuid::Uuid;

impl ReadExecutionDB for SqliteDatabase {
    fn all_trade_executions(&mut self, trade_id: Uuid) -> Result<Vec<Execution>, Box<dyn Error>> {
        WorkerExecution::read_for_trade(&mut lock_connection_or_exit(&self.connection), trade_id)
    }

    fn all_order_executions(&mut self, order_id: Uuid) -> Result<Vec<Execution>, Box<dyn Error>> {
        WorkerExecution::read_for_order(&mut lock_connection_or_exit(&self.connection), order_id)
    }

    fn latest_trade_execution_at(
        &mut self,
        trade_id: Uuid,
    ) -> Result<Option<chrono::NaiveDateTime>, Box<dyn Error>> {
        WorkerExecution::latest_for_trade(&mut lock_connection_or_exit(&self.connection), trade_id)
    }
}

impl WriteExecutionDB for SqliteDatabase {
    fn record_fee_activity(
        &mut self,
        fee: &model::FeeActivity,
        trade_id: Option<Uuid>,
    ) -> Result<bool, Box<dyn Error>> {
        crate::workers::WorkerFee::record(
            &mut lock_connection_or_exit(&self.connection),
            fee,
            trade_id,
        )
    }

    fn upsert_execution(&mut self, execution: &Execution) -> Result<Execution, Box<dyn Error>> {
        WorkerExecution::upsert(&mut lock_connection_or_exit(&self.connection), execution)
    }
}
