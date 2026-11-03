BEGIN IMMEDIATE;
CREATE TABLE broker_statements (
  id TEXT PRIMARY KEY NOT NULL,
  scope TEXT NOT NULL,
  source_hash TEXT NOT NULL,
  timezone TEXT NOT NULL,
  source_xml TEXT NOT NULL,
  metadata TEXT NOT NULL,
  audit_sequence INTEGER UNIQUE NOT NULL REFERENCES audit(sequence),
  UNIQUE(scope, source_hash, timezone)
);
CREATE TABLE statement_reconciliations (
  sequence INTEGER PRIMARY KEY AUTOINCREMENT,
  statement_id TEXT NOT NULL REFERENCES broker_statements(id),
  result TEXT NOT NULL,
  audit_sequence INTEGER UNIQUE NOT NULL REFERENCES audit(sequence)
);
CREATE INDEX statement_reconciliations_latest ON statement_reconciliations(statement_id, sequence DESC);
CREATE INDEX broker_statements_scope ON broker_statements(scope, id);
CREATE TRIGGER broker_statements_no_update BEFORE UPDATE ON broker_statements BEGIN SELECT RAISE(ABORT,'immutable statement source'); END;
CREATE TRIGGER broker_statements_no_delete BEFORE DELETE ON broker_statements BEGIN SELECT RAISE(ABORT,'immutable statement source'); END;
CREATE TRIGGER statement_attempts_no_update BEFORE UPDATE ON statement_reconciliations BEGIN SELECT RAISE(ABORT,'immutable statement attempt'); END;
CREATE TRIGGER statement_attempts_no_delete BEFORE DELETE ON statement_reconciliations BEGIN SELECT RAISE(ABORT,'immutable statement attempt'); END;
PRAGMA user_version = 4;
COMMIT;
