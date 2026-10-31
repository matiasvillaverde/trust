-- A downgrade cannot represent new values <= 1 without changing their meaning.
-- Refuse a lossy downgrade; restore a pre-upgrade backup instead.
CREATE TEMP TABLE percentage_downgrade_guard (safe INTEGER CHECK (safe = 1));
INSERT INTO percentage_downgrade_guard SELECT 0 FROM accounts
    WHERE tax_percentage_units = 'percentage_points' LIMIT 1;
DROP TABLE percentage_downgrade_guard;
ALTER TABLE accounts DROP COLUMN tax_percentage_units;
