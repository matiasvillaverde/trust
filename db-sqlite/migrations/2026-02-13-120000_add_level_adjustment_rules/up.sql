CREATE TABLE level_adjustment_rules (
    id TEXT PRIMARY KEY NOT NULL,
    created_at TIMESTAMP NOT NULL,
    updated_at TIMESTAMP NOT NULL,
    deleted_at TIMESTAMP NULL,
    account_id TEXT NOT NULL UNIQUE,
    monthly_loss_downgrade_pct TEXT NOT NULL,
    single_loss_downgrade_pct TEXT NOT NULL,
    upgrade_profitable_trades INTEGER NOT NULL,
    upgrade_win_rate_pct TEXT NOT NULL,
    upgrade_consecutive_wins INTEGER NOT NULL,
    cooldown_profitable_trades INTEGER NOT NULL,
    cooldown_win_rate_pct TEXT NOT NULL,
    cooldown_consecutive_wins INTEGER NOT NULL,
    recovery_profitable_trades INTEGER NOT NULL,
    recovery_win_rate_pct TEXT NOT NULL,
    recovery_consecutive_wins INTEGER NOT NULL,
    min_trades_at_level_for_upgrade INTEGER NOT NULL,
    max_changes_in_30_days INTEGER NOT NULL,
    FOREIGN KEY(account_id) REFERENCES accounts(id)
);

CREATE INDEX idx_level_adjustment_rules_account_id ON level_adjustment_rules(account_id);
