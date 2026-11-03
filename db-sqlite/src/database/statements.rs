use super::SqliteDatabase;
use model::statement::{ReadStatementDB, StatementLedger};
use std::{collections::BTreeSet, error::Error};
use uuid::Uuid;
impl ReadStatementDB for SqliteDatabase {
    fn read_statement_ledger(
        &mut self,
        accounts: &BTreeSet<Uuid>,
    ) -> Result<StatementLedger, Box<dyn Error>> {
        let mut connection = self
            .connection
            .lock()
            .map_err(|_| "Statement database lock unavailable")?;
        crate::workers::worker_statement::read_statement_ledger(&mut connection, accounts)
    }
}
