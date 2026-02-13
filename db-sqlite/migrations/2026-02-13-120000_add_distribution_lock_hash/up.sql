ALTER TABLE distribution_rules
ADD COLUMN configuration_password_hash TEXT NOT NULL DEFAULT '';
