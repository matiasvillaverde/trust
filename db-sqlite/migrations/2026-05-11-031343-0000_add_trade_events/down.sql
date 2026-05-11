-- This file should undo anything in `up.sql`
DROP INDEX IF EXISTS idx_trade_events_symbol_date_active;
DROP INDEX IF EXISTS idx_trade_events_trade_id_active;
DROP TABLE trade_events;
