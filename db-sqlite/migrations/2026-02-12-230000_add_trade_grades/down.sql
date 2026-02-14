-- SQLite cannot drop columns. We only remove the indexes added by the "up".
DROP INDEX IF EXISTS idx_trade_grades_trade_id;
DROP INDEX IF EXISTS idx_trade_grades_graded_at;
