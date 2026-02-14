-- SQLite does not support DROP COLUMN directly. Recreate table without configuration_password_hash.
CREATE TABLE distribution_rules_tmp (
    id TEXT NOT NULL PRIMARY KEY,
    created_at DATETIME NOT NULL,
    updated_at DATETIME NOT NULL,
    account_id TEXT NOT NULL REFERENCES accounts(id),
    earnings_percent TEXT NOT NULL,
    tax_percent TEXT NOT NULL,
    reinvestment_percent TEXT NOT NULL,
    minimum_threshold TEXT NOT NULL
);

INSERT INTO distribution_rules_tmp (
    id, created_at, updated_at, account_id, earnings_percent, tax_percent, reinvestment_percent, minimum_threshold
)
SELECT
    id, created_at, updated_at, account_id, earnings_percent, tax_percent, reinvestment_percent, minimum_threshold
FROM distribution_rules;

DROP TABLE distribution_rules;
ALTER TABLE distribution_rules_tmp RENAME TO distribution_rules;

CREATE INDEX idx_distribution_rules_account_id ON distribution_rules(account_id);
