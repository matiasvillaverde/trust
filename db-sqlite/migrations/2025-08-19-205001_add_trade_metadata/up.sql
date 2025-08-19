-- Add metadata fields to trades table
ALTER TABLE trades ADD COLUMN thesis TEXT;
ALTER TABLE trades ADD COLUMN sector TEXT;
ALTER TABLE trades ADD COLUMN asset_class TEXT;
ALTER TABLE trades ADD COLUMN context TEXT;