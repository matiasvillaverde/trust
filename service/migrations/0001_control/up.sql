PRAGMA journal_mode = WAL;
PRAGMA synchronous = FULL;
PRAGMA foreign_keys = ON;
CREATE TABLE IF NOT EXISTS operations (
  id TEXT PRIMARY KEY NOT NULL,
  subject TEXT NOT NULL,
  request_key TEXT NOT NULL,
  tool TEXT NOT NULL,
  account_id TEXT,
  account_ids TEXT NOT NULL,
  request_hash TEXT NOT NULL,
  state TEXT NOT NULL CHECK (state IN ('running','completed','failed','outcome_unknown')),
  response TEXT,
  created_at TEXT NOT NULL,
  UNIQUE(subject, request_key)
);
CREATE TABLE IF NOT EXISTS audit (
  sequence INTEGER PRIMARY KEY AUTOINCREMENT,
  event TEXT NOT NULL,
  previous_hash TEXT NOT NULL,
  event_hash TEXT NOT NULL UNIQUE
);
CREATE TRIGGER IF NOT EXISTS audit_no_update BEFORE UPDATE ON audit BEGIN SELECT RAISE(ABORT, 'audit records are append-only'); END;
CREATE TRIGGER IF NOT EXISTS audit_no_delete BEFORE DELETE ON audit BEGIN SELECT RAISE(ABORT, 'audit records are append-only'); END;
PRAGMA user_version = 1;
