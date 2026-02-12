CREATE TABLE levels (
    id TEXT NOT NULL PRIMARY KEY,
    created_at DATETIME NOT NULL,
    updated_at DATETIME NOT NULL,
    deleted_at DATETIME,
    account_id TEXT NOT NULL REFERENCES accounts(id),
    current_level INTEGER NOT NULL CHECK (current_level >= 0 AND current_level <= 4),
    risk_multiplier TEXT NOT NULL,
    status TEXT NOT NULL CHECK (status IN ('normal', 'probation', 'cooldown')),
    trades_at_level INTEGER NOT NULL DEFAULT 0,
    level_start_date DATE NOT NULL
);

CREATE TABLE level_changes (
    id TEXT NOT NULL PRIMARY KEY,
    created_at DATETIME NOT NULL,
    updated_at DATETIME NOT NULL,
    deleted_at DATETIME,
    account_id TEXT NOT NULL REFERENCES accounts(id),
    old_level INTEGER NOT NULL CHECK (old_level >= 0 AND old_level <= 4),
    new_level INTEGER NOT NULL CHECK (new_level >= 0 AND new_level <= 4),
    change_reason TEXT NOT NULL,
    trigger_type TEXT NOT NULL,
    changed_at DATETIME NOT NULL
);

CREATE UNIQUE INDEX idx_levels_account ON levels(account_id) WHERE deleted_at IS NULL;
CREATE INDEX idx_level_changes_account ON level_changes(account_id);
CREATE INDEX idx_level_changes_date ON level_changes(changed_at);