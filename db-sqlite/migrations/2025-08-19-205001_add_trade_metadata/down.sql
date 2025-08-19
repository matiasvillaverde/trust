-- Remove metadata fields from trades table
-- Note: SQLite doesn't support DROP COLUMN directly, need to recreate table
CREATE TABLE trades_new (
    id TEXT NOT NULL PRIMARY KEY,
    created_at DATETIME NOT NULL,
    updated_at DATETIME NOT NULL,
    deleted_at DATETIME,
    category TEXT CHECK(category IN ('long', 'short')) NOT NULL,
    status TEXT CHECK(status IN ('new', 'funded', 'submitted', 'partially_filled', 'filled', 'canceled', 'expired', 'rejected', 'closed_stop_loss', 'closed_target')) NOT NULL,
    currency TEXT CHECK(currency IN ('USD', 'EUR', 'BTC')) NOT NULL,
    trading_vehicle_id TEXT NOT NULL REFERENCES trading_vehicles(id),
    safety_stop_id TEXT NOT NULL REFERENCES orders(id),
    entry_id TEXT NOT NULL REFERENCES orders(id),
    target_id TEXT NOT NULL REFERENCES orders(id),
    account_id TEXT NOT NULL REFERENCES accounts(id),
    balance_id TEXT NOT NULL REFERENCES trades_balances(id)
);

-- Copy data from old table to new
INSERT INTO trades_new SELECT 
    id, created_at, updated_at, deleted_at, category, status, currency,
    trading_vehicle_id, safety_stop_id, entry_id, target_id, account_id, balance_id
FROM trades;

-- Drop old table
DROP TABLE trades;

-- Rename new table
ALTER TABLE trades_new RENAME TO trades;