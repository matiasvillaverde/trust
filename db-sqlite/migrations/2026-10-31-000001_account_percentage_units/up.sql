-- Preserve old tax-rate interpretation without floating-point conversion.
-- The adapter translates legacy fractions exactly; new writes use percentage points.
ALTER TABLE accounts ADD COLUMN tax_percentage_units TEXT NOT NULL DEFAULT 'legacy'
    CHECK (tax_percentage_units IN ('legacy', 'percentage_points'));
