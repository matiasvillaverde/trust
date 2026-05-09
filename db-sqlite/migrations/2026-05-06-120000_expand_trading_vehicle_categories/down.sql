-- Revert category CHECK constraint to the legacy stock/crypto/fiat set.
-- Existing etf/bond rows are conservatively mapped to stock so rollback can
-- complete without data loss from failed CHECK constraints.

PRAGMA foreign_keys=OFF;

CREATE TABLE "trading_vehicles_new" (
    id              TEXT NOT NULL PRIMARY KEY,
    created_at      DATETIME NOT NULL,
    updated_at      DATETIME NOT NULL,
    deleted_at      DATETIME,

    symbol          TEXT NOT NULL,
    isin            TEXT,
    category        TEXT CHECK(category IN ('crypto', 'fiat', 'stock')) NOT NULL,
    broker          TEXT NOT NULL,

    broker_asset_id     TEXT,
    exchange            TEXT,
    broker_asset_class  TEXT,
    broker_asset_status TEXT,
    tradable            BOOLEAN,
    marginable          BOOLEAN,
    shortable           BOOLEAN,
    easy_to_borrow      BOOLEAN,
    fractionable        BOOLEAN
);

INSERT INTO trading_vehicles_new (
    id, created_at, updated_at, deleted_at,
    symbol, isin, category, broker,
    broker_asset_id, exchange, broker_asset_class, broker_asset_status,
    tradable, marginable, shortable, easy_to_borrow, fractionable
)
SELECT
    id, created_at, updated_at, deleted_at,
    symbol,
    isin,
    CASE
        WHEN category IN ('crypto', 'fiat', 'stock') THEN category
        ELSE 'stock'
    END,
    broker,
    broker_asset_id, exchange, broker_asset_class, broker_asset_status,
    tradable, marginable, shortable, easy_to_borrow, fractionable
FROM trading_vehicles;

DROP TABLE trading_vehicles;
ALTER TABLE trading_vehicles_new RENAME TO trading_vehicles;

CREATE UNIQUE INDEX trading_vehicles_broker_symbol_unique
ON trading_vehicles (broker, symbol);

CREATE UNIQUE INDEX trading_vehicles_broker_asset_id_unique
ON trading_vehicles (broker, broker_asset_id)
WHERE broker_asset_id IS NOT NULL;

PRAGMA foreign_keys=ON;
