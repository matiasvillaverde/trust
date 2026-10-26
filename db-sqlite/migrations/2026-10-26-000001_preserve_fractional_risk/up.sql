-- Preserve the legacy integer column for old rows and backup compatibility.
-- New writers populate an exact decimal string instead of truncating percentages.
ALTER TABLE rules ADD COLUMN risk_decimal TEXT;
