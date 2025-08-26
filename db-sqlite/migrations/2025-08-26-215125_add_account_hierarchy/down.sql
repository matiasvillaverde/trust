-- Remove indices first
DROP INDEX IF EXISTS idx_accounts_account_type;
DROP INDEX IF EXISTS idx_accounts_parent_id;

-- Remove added columns (SQLite doesn't support DROP COLUMN before version 3.35.0)
-- In SQLite, we need to recreate the table without these columns
-- This is a destructive operation that would lose hierarchy data
-- For production, consider keeping the columns with NULL values instead

-- For this migration, we'll keep the columns but set them to defaults
UPDATE accounts SET account_type = 'primary' WHERE account_type IS NOT NULL;
UPDATE accounts SET parent_account_id = NULL;