-- Never silently lose a fractional risk policy on downgrade.
CREATE TEMP TABLE risk_downgrade_guard (fractional_rows INTEGER CHECK (fractional_rows = 0));
INSERT INTO risk_downgrade_guard SELECT COUNT(*) FROM rules
WHERE risk_decimal IS NOT NULL AND CAST(risk_decimal AS NUMERIC) != risk;
DROP TABLE risk_downgrade_guard;
ALTER TABLE rules DROP COLUMN risk_decimal;
