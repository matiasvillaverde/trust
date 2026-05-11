-- This file should undo anything in `up.sql`
DROP TRIGGER IF EXISTS trg_session_plans_review_only_on_close;
DROP TRIGGER IF EXISTS trg_session_plans_immutable_plan_fields;
DROP INDEX IF EXISTS idx_session_plans_account_opened_active;
DROP INDEX IF EXISTS idx_session_plans_one_open_per_account;
DROP TABLE session_plans;
