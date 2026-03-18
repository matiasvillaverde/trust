ALTER TABLE accounts ADD COLUMN broker_kind TEXT NOT NULL DEFAULT 'alpaca';
ALTER TABLE accounts ADD COLUMN broker_account_id TEXT;

CREATE INDEX idx_accounts_broker_kind ON accounts(broker_kind);
CREATE INDEX idx_accounts_broker_account_id ON accounts(broker_account_id);
