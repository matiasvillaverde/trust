BEGIN IMMEDIATE;
CREATE INDEX account_safety_history_scope_sequence
  ON account_safety_history(scope, audit_sequence DESC);
CREATE INDEX operations_unfinished
  ON operations(state) WHERE state = 'running' OR state = 'outcome_unknown';
PRAGMA user_version = 3;
COMMIT;
