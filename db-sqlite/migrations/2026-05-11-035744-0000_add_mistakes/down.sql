-- This file should undo anything in `up.sql`
DROP INDEX IF EXISTS idx_mistakes_created_at_active;
DROP INDEX IF EXISTS idx_mistakes_trade_id_active;
DROP TABLE mistakes;
