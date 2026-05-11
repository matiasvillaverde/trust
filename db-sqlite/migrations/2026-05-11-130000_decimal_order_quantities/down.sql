PRAGMA foreign_keys=OFF;

CREATE TABLE orders_old (
    id                    TEXT PRIMARY KEY NOT NULL,
    broker_order_id       TEXT,
    created_at            TIMESTAMP NOT NULL,
    updated_at            TIMESTAMP NOT NULL,
    deleted_at            TIMESTAMP,
    unit_price            TEXT NOT NULL,
    currency              TEXT NOT NULL,
    quantity              INTEGER NOT NULL,
    category              TEXT NOT NULL CHECK (category IN ('market', 'limit', 'stop', 'stop_limit')),
    trading_vehicle_id    TEXT NOT NULL,
    action                TEXT NOT NULL,
    status                TEXT NOT NULL,
    time_in_force         TEXT NOT NULL,
    trailing_percentage   TEXT,
    trailing_price        TEXT,
    filled_quantity       INTEGER,
    average_filled_price  TEXT,
    extended_hours        BOOLEAN NOT NULL DEFAULT 0,
    submitted_at          TIMESTAMP,
    filled_at             TIMESTAMP,
    expired_at            TIMESTAMP,
    cancelled_at          TIMESTAMP,
    closed_at             TIMESTAMP,
    FOREIGN KEY(trading_vehicle_id) REFERENCES trading_vehicles(id)
);

INSERT INTO orders_old (
    id, broker_order_id, created_at, updated_at, deleted_at,
    unit_price, currency, quantity, category, trading_vehicle_id,
    action, status, time_in_force, trailing_percentage, trailing_price,
    filled_quantity, average_filled_price, extended_hours, submitted_at,
    filled_at, expired_at, cancelled_at, closed_at
)
SELECT
    id, broker_order_id, created_at, updated_at, deleted_at,
    unit_price, currency, CAST(quantity AS INTEGER), category, trading_vehicle_id,
    action, status, time_in_force, trailing_percentage, trailing_price,
    CASE WHEN filled_quantity IS NULL THEN NULL ELSE CAST(filled_quantity AS INTEGER) END,
    average_filled_price, extended_hours, submitted_at,
    filled_at, expired_at, cancelled_at, closed_at
FROM orders;

DROP TABLE orders;
ALTER TABLE orders_old RENAME TO orders;

PRAGMA foreign_key_check;
PRAGMA foreign_keys=ON;
