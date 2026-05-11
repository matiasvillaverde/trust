-- Add stop_limit as a supported order category.
--
-- SQLite cannot alter CHECK constraints in place, so rebuild the orders table
-- while preserving every persisted order field.

PRAGMA foreign_keys=OFF;

CREATE TABLE "orders_new" (
    id                    TEXT NOT NULL PRIMARY KEY,
    broker_order_id       TEXT,
    created_at            DATETIME NOT NULL,
    updated_at            DATETIME NOT NULL,
    deleted_at            DATETIME,
    unit_price            TEXT NOT NULL,
    currency              TEXT CHECK(currency IN ('USD', 'EUR', 'BTC')) NOT NULL,
    quantity              INTEGER NOT NULL,
    category              TEXT CHECK(category IN ('market', 'limit', 'stop', 'stop_limit')) NOT NULL,
    trading_vehicle_id    TEXT NOT NULL REFERENCES trading_vehicles (id),
    action                TEXT CHECK(action IN ('sell', 'buy', 'short')) NOT NULL,
    status                TEXT CHECK(status IN ('new', 'replaced', 'partially_filled', 'filled', 'done_for_day', 'canceled', 'expired', 'accepted', 'pending_new', 'accepted_for_bidding', 'pending_cancel', 'pending_replace', 'stopped', 'rejected', 'suspended', 'calculated', 'held', 'unknown')) NOT NULL,
    time_in_force         TEXT CHECK(time_in_force IN ('until_canceled', 'day', 'until_market_open', 'until_market_close')) NOT NULL,
    trailing_percentage   TEXT,
    trailing_price        TEXT,
    filled_quantity       INTEGER,
    average_filled_price  TEXT,
    extended_hours        BOOLEAN NOT NULL,
    submitted_at          DATETIME,
    filled_at             DATETIME,
    expired_at            DATETIME,
    cancelled_at          DATETIME,
    closed_at             DATETIME
);

INSERT INTO orders_new (
    id, broker_order_id, created_at, updated_at, deleted_at,
    unit_price, currency, quantity, category, trading_vehicle_id,
    action, status, time_in_force, trailing_percentage, trailing_price,
    filled_quantity, average_filled_price, extended_hours, submitted_at,
    filled_at, expired_at, cancelled_at, closed_at
)
SELECT
    id, broker_order_id, created_at, updated_at, deleted_at,
    unit_price, currency, quantity, category, trading_vehicle_id,
    action, status, time_in_force, trailing_percentage, trailing_price,
    filled_quantity, average_filled_price, extended_hours, submitted_at,
    filled_at, expired_at, cancelled_at, closed_at
FROM orders;

DROP TABLE orders;
ALTER TABLE orders_new RENAME TO orders;

PRAGMA foreign_keys=ON;
