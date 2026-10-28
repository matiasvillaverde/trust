BEGIN IMMEDIATE;
DROP TABLE IF EXISTS account_safety_history;
DROP TABLE operation_resolutions;
DROP TABLE account_safety;
PRAGMA user_version = 1;
COMMIT;
