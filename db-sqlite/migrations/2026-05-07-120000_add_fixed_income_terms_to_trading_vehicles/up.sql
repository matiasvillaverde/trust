-- Add optional fixed-income terms to trading vehicles.
--
-- Values are nullable because broker metadata may identify a bond before Trust
-- has coupon/par/maturity enrichment.

ALTER TABLE trading_vehicles ADD COLUMN fixed_income_face_value TEXT;
ALTER TABLE trading_vehicles ADD COLUMN fixed_income_coupon_rate_pct TEXT;
ALTER TABLE trading_vehicles ADD COLUMN fixed_income_maturity_date DATE;
ALTER TABLE trading_vehicles ADD COLUMN fixed_income_coupon_frequency_per_year INTEGER;
