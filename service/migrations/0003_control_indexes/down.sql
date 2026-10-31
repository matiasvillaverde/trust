BEGIN IMMEDIATE;
DROP INDEX operations_unfinished;
DROP INDEX account_safety_history_scope_sequence;
PRAGMA user_version = 2;
COMMIT;
