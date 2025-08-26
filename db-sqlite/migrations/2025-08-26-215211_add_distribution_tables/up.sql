-- Create distribution_rules table
CREATE TABLE distribution_rules (
    id TEXT NOT NULL PRIMARY KEY,
    created_at DATETIME NOT NULL,
    updated_at DATETIME NOT NULL,
    account_id TEXT NOT NULL REFERENCES accounts(id),
    earnings_percent TEXT NOT NULL,
    tax_percent TEXT NOT NULL, 
    reinvestment_percent TEXT NOT NULL,
    minimum_threshold TEXT NOT NULL
);

-- Create distribution_history table
CREATE TABLE distribution_history (
    id TEXT NOT NULL PRIMARY KEY,
    created_at DATETIME NOT NULL,
    updated_at DATETIME NOT NULL,
    source_account_id TEXT NOT NULL REFERENCES accounts(id),
    trade_id TEXT REFERENCES trades(id), -- NULL for manual distributions
    original_amount TEXT NOT NULL,
    distribution_date DATETIME NOT NULL,
    earnings_amount TEXT,
    tax_amount TEXT,
    reinvestment_amount TEXT
);

-- Add indices for efficient queries
CREATE INDEX idx_distribution_rules_account_id ON distribution_rules(account_id);
CREATE INDEX idx_distribution_history_source_account ON distribution_history(source_account_id);
CREATE INDEX idx_distribution_history_trade ON distribution_history(trade_id);
CREATE INDEX idx_distribution_history_date ON distribution_history(distribution_date);