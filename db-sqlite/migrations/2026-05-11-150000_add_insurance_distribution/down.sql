DROP INDEX IF EXISTS idx_distribution_rules_account_id_unique;
DROP INDEX IF EXISTS idx_distribution_rules_account_id;

CREATE TABLE distribution_rules_tmp (
    id TEXT NOT NULL PRIMARY KEY,
    created_at DATETIME NOT NULL,
    updated_at DATETIME NOT NULL,
    account_id TEXT NOT NULL REFERENCES accounts(id),
    earnings_percent TEXT NOT NULL,
    tax_percent TEXT NOT NULL,
    reinvestment_percent TEXT NOT NULL,
    minimum_threshold TEXT NOT NULL,
    configuration_password_hash TEXT NOT NULL
);

INSERT INTO distribution_rules_tmp (
    id,
    created_at,
    updated_at,
    account_id,
    earnings_percent,
    tax_percent,
    reinvestment_percent,
    minimum_threshold,
    configuration_password_hash
)
SELECT
    id,
    created_at,
    updated_at,
    account_id,
    earnings_percent,
    tax_percent,
    reinvestment_percent,
    minimum_threshold,
    configuration_password_hash
FROM distribution_rules;

DROP TABLE distribution_rules;
ALTER TABLE distribution_rules_tmp RENAME TO distribution_rules;

CREATE INDEX idx_distribution_rules_account_id ON distribution_rules(account_id);
CREATE UNIQUE INDEX idx_distribution_rules_account_id_unique ON distribution_rules(account_id);

DROP INDEX IF EXISTS idx_distribution_history_date;
DROP INDEX IF EXISTS idx_distribution_history_trade;
DROP INDEX IF EXISTS idx_distribution_history_source_account;

CREATE TABLE distribution_history_tmp (
    id TEXT NOT NULL PRIMARY KEY,
    created_at DATETIME NOT NULL,
    updated_at DATETIME NOT NULL,
    source_account_id TEXT NOT NULL REFERENCES accounts(id),
    trade_id TEXT REFERENCES trades(id),
    original_amount TEXT NOT NULL,
    distribution_date DATETIME NOT NULL,
    earnings_amount TEXT,
    tax_amount TEXT,
    reinvestment_amount TEXT
);

INSERT INTO distribution_history_tmp (
    id,
    created_at,
    updated_at,
    source_account_id,
    trade_id,
    original_amount,
    distribution_date,
    earnings_amount,
    tax_amount,
    reinvestment_amount
)
SELECT
    id,
    created_at,
    updated_at,
    source_account_id,
    trade_id,
    original_amount,
    distribution_date,
    earnings_amount,
    tax_amount,
    reinvestment_amount
FROM distribution_history;

DROP TABLE distribution_history;
ALTER TABLE distribution_history_tmp RENAME TO distribution_history;

CREATE INDEX idx_distribution_history_source_account ON distribution_history(source_account_id);
CREATE INDEX idx_distribution_history_trade ON distribution_history(trade_id);
CREATE INDEX idx_distribution_history_date ON distribution_history(distribution_date);
