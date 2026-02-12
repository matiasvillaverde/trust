-- Refactor trading_vehicles to support broker-provided metadata and broker-scoped identity.
--
-- Why rebuild:
-- 1) We need to drop legacy global UNIQUE(isin) semantics from the original table definition.
-- 2) We avoid `ALTER TABLE ... RENAME TO trading_vehicles_old` on a referenced table, because
--    SQLite may rewrite dependent foreign keys to the temporary name.
--
-- Safe swap pattern:
-- - Create `trading_vehicles_new`
-- - Copy data
-- - Drop old table
-- - Rename new table to `trading_vehicles`
--
-- Note: We disable FK checks during the swap only.
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

    -- Broker-provided metadata
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
    symbol, isin, category, broker
)
SELECT
    id, created_at, updated_at, deleted_at,
    symbol, isin, category, broker
FROM trading_vehicles;

DROP TABLE trading_vehicles;
ALTER TABLE trading_vehicles_new RENAME TO trading_vehicles;

-- New identity: symbol within broker.
CREATE UNIQUE INDEX trading_vehicles_broker_symbol_unique
ON trading_vehicles (broker, symbol);

-- Optional broker global id, unique within broker when present.
CREATE UNIQUE INDEX trading_vehicles_broker_asset_id_unique
ON trading_vehicles (broker, broker_asset_id)
WHERE broker_asset_id IS NOT NULL;

PRAGMA foreign_keys=ON;

