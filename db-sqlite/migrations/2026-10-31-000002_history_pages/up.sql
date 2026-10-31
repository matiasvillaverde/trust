CREATE INDEX idx_trade_history_page ON trades(account_id, created_at, id) WHERE deleted_at IS NULL;
CREATE INDEX idx_trade_status_page ON trades(account_id, status, created_at, id) WHERE deleted_at IS NULL;
CREATE INDEX idx_transaction_history_page ON transactions(account_id, created_at, id) WHERE deleted_at IS NULL;
CREATE INDEX idx_distribution_history_page ON distribution_history(source_account_id, distribution_date DESC, id DESC);
