BEGIN IMMEDIATE;
CREATE TABLE IF NOT EXISTS account_safety (
  scope TEXT PRIMARY KEY NOT NULL,
  state TEXT NOT NULL,
  audit_sequence INTEGER NOT NULL REFERENCES audit(sequence)
);
CREATE TABLE IF NOT EXISTS operation_resolutions (
  operation_id TEXT PRIMARY KEY NOT NULL REFERENCES operations(id),
  evidence TEXT NOT NULL,
  audit_sequence INTEGER NOT NULL REFERENCES audit(sequence)
);
CREATE TABLE IF NOT EXISTS account_safety_history (
  audit_sequence INTEGER PRIMARY KEY NOT NULL REFERENCES audit(sequence),
  scope TEXT NOT NULL,
  state TEXT NOT NULL
);
PRAGMA user_version = 2;
COMMIT;
