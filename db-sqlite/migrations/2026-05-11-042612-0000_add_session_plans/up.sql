-- Your SQL goes here
CREATE TABLE session_plans (
    id TEXT NOT NULL PRIMARY KEY,
    created_at DATETIME NOT NULL,
    updated_at DATETIME NOT NULL,
    deleted_at DATETIME,
    account_id TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    opened_at DATETIME NOT NULL,
    closed_at DATETIME,
    regime TEXT NOT NULL CHECK (regime IN ('calm', 'normal', 'elevated')),
    permitted_setups TEXT NOT NULL CHECK (length(trim(permitted_setups)) > 0),
    max_positions INTEGER NOT NULL CHECK (max_positions >= 0),
    hypothesis TEXT NOT NULL CHECK (
        length(trim(hypothesis)) > 0
        AND length(hypothesis) <= 500
    ),
    success_criteria TEXT NOT NULL CHECK (length(trim(success_criteria)) > 0),
    failure_criteria TEXT NOT NULL CHECK (length(trim(failure_criteria)) > 0),
    session_grade TEXT CHECK (
        session_grade IS NULL
        OR length(trim(session_grade)) > 0
    ),
    adherence_notes TEXT CHECK (
        adherence_notes IS NULL
        OR length(trim(adherence_notes)) > 0
    ),
    CHECK (closed_at IS NULL OR closed_at >= opened_at),
    CHECK (
        closed_at IS NOT NULL
        OR (session_grade IS NULL AND adherence_notes IS NULL)
    )
);

CREATE UNIQUE INDEX idx_session_plans_one_open_per_account
ON session_plans(account_id)
WHERE closed_at IS NULL AND deleted_at IS NULL;

CREATE INDEX idx_session_plans_account_opened_active
ON session_plans(account_id, opened_at)
WHERE deleted_at IS NULL;

CREATE TRIGGER trg_session_plans_immutable_plan_fields
BEFORE UPDATE ON session_plans
WHEN
    OLD.account_id IS NOT NEW.account_id
    OR OLD.opened_at IS NOT NEW.opened_at
    OR OLD.regime IS NOT NEW.regime
    OR OLD.permitted_setups IS NOT NEW.permitted_setups
    OR OLD.max_positions IS NOT NEW.max_positions
    OR OLD.hypothesis IS NOT NEW.hypothesis
    OR OLD.success_criteria IS NOT NEW.success_criteria
    OR OLD.failure_criteria IS NOT NEW.failure_criteria
BEGIN
    SELECT RAISE(ABORT, 'session plan fields are immutable after open');
END;

CREATE TRIGGER trg_session_plans_review_only_on_close
BEFORE UPDATE ON session_plans
WHEN
    (
        OLD.closed_at IS NOT NEW.closed_at
        OR OLD.session_grade IS NOT NEW.session_grade
        OR OLD.adherence_notes IS NOT NEW.adherence_notes
    )
    AND NOT (OLD.closed_at IS NULL AND NEW.closed_at IS NOT NULL)
BEGIN
    SELECT RAISE(ABORT, 'session review fields can only be set when closing session');
END;
