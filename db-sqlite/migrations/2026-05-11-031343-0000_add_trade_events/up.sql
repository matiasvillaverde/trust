-- Your SQL goes here
-- Add scheduled and discretionary catalysts associated with trades.
CREATE TABLE trade_events (
    id TEXT NOT NULL PRIMARY KEY,
    created_at DATETIME NOT NULL,
    updated_at DATETIME NOT NULL,
    deleted_at DATETIME,

    trade_id TEXT NOT NULL REFERENCES trades(id) ON DELETE CASCADE,
    symbol TEXT NOT NULL,
    event_type TEXT NOT NULL CHECK (
        event_type IN (
            'earnings',
            'fed',
            'cpi',
            'nfp',
            'ex_dividend',
            'guidance',
            'other'
        )
    ),
    event_date DATE NOT NULL,
    severity TEXT NOT NULL CHECK (severity IN ('low', 'medium', 'high')),
    notes TEXT,
    source TEXT NOT NULL CHECK (source IN ('manual', 'calendar_api'))
);

CREATE INDEX idx_trade_events_trade_id_active
    ON trade_events(trade_id, event_date)
    WHERE deleted_at IS NULL;

CREATE INDEX idx_trade_events_symbol_date_active
    ON trade_events(symbol, event_date)
    WHERE deleted_at IS NULL;
