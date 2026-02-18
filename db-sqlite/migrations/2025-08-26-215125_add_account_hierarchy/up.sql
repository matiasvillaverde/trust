-- Add account hierarchy fields to existing accounts table
ALTER TABLE accounts ADD COLUMN account_type TEXT NOT NULL DEFAULT 'primary';
ALTER TABLE accounts ADD COLUMN parent_account_id TEXT REFERENCES accounts(id);

-- Add indices for efficient hierarchy queries
CREATE INDEX idx_accounts_parent_id ON accounts(parent_account_id);
CREATE INDEX idx_accounts_account_type ON accounts(account_type);