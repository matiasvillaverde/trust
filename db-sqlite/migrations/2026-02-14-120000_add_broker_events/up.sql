CREATE TABLE broker_events (
    id          TEXT NOT NULL PRIMARY KEY,
    created_at  DATETIME NOT NULL,
    updated_at  DATETIME NOT NULL,
    deleted_at  DATETIME,

    account_id  TEXT NOT NULL REFERENCES accounts(id),
    trade_id    TEXT NOT NULL REFERENCES trades(id),

    source      TEXT NOT NULL,
    stream      TEXT NOT NULL,
    event_type  TEXT NOT NULL,

    broker_order_id TEXT,

    payload_json TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_broker_events_trade_created_active
ON broker_events(trade_id, created_at)
WHERE deleted_at IS NULL;

CREATE INDEX IF NOT EXISTS idx_broker_events_account_created_active
ON broker_events(account_id, created_at)
WHERE deleted_at IS NULL;
