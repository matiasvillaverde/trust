-- Your SQL goes here
-- Add structured post-trade bias analysis linked to trades.
CREATE TABLE mistakes (
    id TEXT NOT NULL PRIMARY KEY,
    created_at DATETIME NOT NULL,
    updated_at DATETIME NOT NULL,
    deleted_at DATETIME,

    trade_id TEXT NOT NULL REFERENCES trades(id) ON DELETE CASCADE,
    bias_tags TEXT NOT NULL CHECK (length(trim(bias_tags)) > 0),
    lollapalooza BOOLEAN NOT NULL,
    error_type TEXT NOT NULL CHECK (error_type IN ('commission', 'omission')),
    rule_violated TEXT,
    counterfactual_r TEXT NOT NULL,
    lesson TEXT NOT NULL CHECK (length(trim(lesson)) > 0)
);

CREATE INDEX idx_mistakes_trade_id_active
    ON mistakes(trade_id, created_at)
    WHERE deleted_at IS NULL;

CREATE INDEX idx_mistakes_created_at_active
    ON mistakes(created_at)
    WHERE deleted_at IS NULL;
