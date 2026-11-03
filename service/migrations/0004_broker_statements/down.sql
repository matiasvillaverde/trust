BEGIN IMMEDIATE;
DROP TABLE statement_reconciliations;
DROP TABLE broker_statements;
PRAGMA user_version = 3;
COMMIT;
