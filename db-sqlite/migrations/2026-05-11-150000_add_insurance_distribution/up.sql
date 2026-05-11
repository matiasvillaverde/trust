ALTER TABLE distribution_rules
ADD COLUMN insurance_percent TEXT NOT NULL DEFAULT '0';

ALTER TABLE distribution_history
ADD COLUMN insurance_amount TEXT;
