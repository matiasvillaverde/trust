DROP INDEX IF EXISTS idx_accounts_broker_account_id;
DROP INDEX IF EXISTS idx_accounts_broker_kind;

UPDATE accounts SET broker_kind = 'alpaca' WHERE broker_kind IS NOT NULL;
UPDATE accounts SET broker_account_id = NULL;
