PRAGMA foreign_keys=OFF;

CREATE TABLE trades_old (
    id                 TEXT NOT NULL PRIMARY KEY,
    created_at         DATETIME NOT NULL,
    updated_at         DATETIME NOT NULL,
    deleted_at         DATETIME,
    category           TEXT CHECK(category IN ('long', 'short')) NOT NULL,
    status             TEXT CHECK(status IN ('new', 'funded', 'submitted', 'partially_filled', 'filled', 'canceled', 'expired', 'rejected', 'closed_stop_loss', 'closed_target')) NOT NULL,
    currency           TEXT CHECK(currency IN ('USD', 'EUR', 'BTC')) NOT NULL,
    trading_vehicle_id TEXT NOT NULL REFERENCES trading_vehicles(id),
    safety_stop_id     TEXT NOT NULL REFERENCES orders(id),
    entry_id           TEXT NOT NULL REFERENCES orders(id),
    target_id          TEXT NOT NULL REFERENCES orders(id),
    account_id         TEXT NOT NULL REFERENCES accounts(id),
    balance_id         TEXT NOT NULL REFERENCES trades_balances(id),
    thesis             TEXT,
    sector             TEXT,
    asset_class        TEXT,
    context            TEXT
);

INSERT INTO trades_old (
    id, created_at, updated_at, deleted_at, category, status, currency,
    trading_vehicle_id, safety_stop_id, entry_id, target_id, account_id,
    balance_id, thesis, sector, asset_class, context
)
SELECT
    id, created_at, updated_at, deleted_at, category, status, currency,
    trading_vehicle_id, safety_stop_id, entry_id, target_id, account_id,
    balance_id, thesis, sector, asset_class, context
FROM trades;

DROP TABLE trades;
ALTER TABLE trades_old RENAME TO trades;

CREATE INDEX IF NOT EXISTS idx_trades_account_status_currency_active
ON trades(account_id, status, currency)
WHERE deleted_at IS NULL;

PRAGMA foreign_key_check;
PRAGMA foreign_keys=ON;
