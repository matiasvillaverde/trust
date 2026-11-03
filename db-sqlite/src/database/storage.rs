//! Connection settings for financial storage and ephemeral verified checkpoints.
use diesel::{prelude::*, sql_query};

pub(super) fn configure(connection: &mut SqliteConnection) -> Result<(), crate::StorageError> {
    for statement in [
        "PRAGMA temp_store = MEMORY;",
        "PRAGMA foreign_keys = ON;",
        "CREATE INDEX IF NOT EXISTS idx_transactions_account_currency_category_active \
         ON transactions(account_id, currency, category, created_at) \
         WHERE deleted_at IS NULL",
        "CREATE INDEX IF NOT EXISTS idx_transactions_trade_category_active \
         ON transactions(trade_id, category, created_at) \
         WHERE deleted_at IS NULL",
        "CREATE INDEX IF NOT EXISTS idx_trades_account_status_currency_active \
         ON trades(account_id, status, currency) \
         WHERE deleted_at IS NULL",
        "CREATE INDEX IF NOT EXISTS idx_accounts_balances_account_currency_active \
         ON accounts_balances(account_id, currency) \
         WHERE deleted_at IS NULL",
        "CREATE INDEX IF NOT EXISTS idx_logs_trade_active \
         ON logs(trade_id, created_at) \
         WHERE deleted_at IS NULL",
    ] {
        sql_query(statement)
            .execute(connection)
            .map_err(|_| crate::StorageError::Database)?;
    }
    Ok(())
}
