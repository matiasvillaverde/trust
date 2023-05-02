// Validate that the transaction is possible
// Create transaction
// Update Account Overview

use trust_model::Database;
use uuid::Uuid;

pub struct TransactionWorker;

impl TransactionWorker {
    fn update_overview(account_id: Uuid, database: &mut dyn Database) {
        let account = database.read_account_id(account_id);
    }
}
