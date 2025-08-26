-- Remove indices first
DROP INDEX IF EXISTS idx_distribution_history_date;
DROP INDEX IF EXISTS idx_distribution_history_trade;
DROP INDEX IF EXISTS idx_distribution_history_source_account;
DROP INDEX IF EXISTS idx_distribution_rules_account_id;

-- Drop tables
DROP TABLE IF EXISTS distribution_history;
DROP TABLE IF EXISTS distribution_rules;