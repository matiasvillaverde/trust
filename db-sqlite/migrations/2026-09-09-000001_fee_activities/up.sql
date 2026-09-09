CREATE TABLE fee_activities (
    id TEXT PRIMARY KEY NOT NULL,
    broker TEXT NOT NULL,
    account_id TEXT NOT NULL REFERENCES accounts(id),
    broker_activity_id TEXT NOT NULL,
    broker_order_id TEXT,
    amount TEXT NOT NULL,
    occurred_at TIMESTAMP NOT NULL,
    trade_id TEXT REFERENCES trades(id),
    UNIQUE (broker, account_id, broker_activity_id)
);
