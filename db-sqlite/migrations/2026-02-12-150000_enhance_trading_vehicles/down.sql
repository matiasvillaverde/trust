-- Down migration: remove broker metadata columns and restore legacy schema with
-- NOT NULL UNIQUE ISIN.
--
-- Use the same safe swap pattern (create/copy/drop/rename) and avoid renaming
-- the referenced table to a temporary name.
PRAGMA foreign_keys=OFF;

CREATE TABLE "trading_vehicles_new" (
    id          TEXT NOT NULL PRIMARY KEY,
    created_at  DATETIME NOT NULL,
    updated_at  DATETIME NOT NULL,
    deleted_at  DATETIME,
    symbol      TEXT NOT NULL,
    isin        TEXT NOT NULL UNIQUE,
    category    TEXT CHECK(category IN ('crypto', 'fiat', 'stock')) NOT NULL,
    broker      TEXT NOT NULL
);

INSERT INTO trading_vehicles_new (
    id, created_at, updated_at, deleted_at,
    symbol, isin, category, broker
)
SELECT
    id, created_at, updated_at, deleted_at,
    symbol,
    COALESCE(isin, broker || ':' || symbol),
    category, broker
FROM trading_vehicles;

DROP TABLE trading_vehicles;
ALTER TABLE trading_vehicles_new RENAME TO trading_vehicles;

PRAGMA foreign_keys=ON;

