CREATE TABLE advisory_thresholds (
    id TEXT NOT NULL PRIMARY KEY,
    created_at DATETIME NOT NULL,
    updated_at DATETIME NOT NULL,
    account_id TEXT NOT NULL REFERENCES accounts(id),
    sector_limit_pct TEXT NOT NULL,
    asset_class_limit_pct TEXT NOT NULL,
    single_position_limit_pct TEXT NOT NULL
);

CREATE UNIQUE INDEX idx_advisory_thresholds_account_id
ON advisory_thresholds(account_id);
