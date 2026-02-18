-- NOTE: trade_grades table exists from an earlier migration but had a restrictive
-- CHECK constraint on `overall_grade` (A/B/C/D/F only). The model supports plus/minus
-- grades, so we rebuild the table to relax the constraint and to add weight columns
-- for reproducible scoring.

PRAGMA foreign_keys=OFF;

CREATE TABLE trade_grades_new (
    id TEXT NOT NULL PRIMARY KEY,
    created_at DATETIME NOT NULL,
    updated_at DATETIME NOT NULL,
    deleted_at DATETIME,

    trade_id TEXT NOT NULL REFERENCES trades(id) ON DELETE CASCADE,

    overall_score INTEGER NOT NULL CHECK (overall_score >= 0 AND overall_score <= 100),
    overall_grade TEXT NOT NULL,
    process_score INTEGER NOT NULL CHECK (process_score >= 0 AND process_score <= 100),
    risk_score INTEGER NOT NULL CHECK (risk_score >= 0 AND risk_score <= 100),
    execution_score INTEGER NOT NULL CHECK (execution_score >= 0 AND execution_score <= 100),
    documentation_score INTEGER NOT NULL CHECK (documentation_score >= 0 AND documentation_score <= 100),

    -- JSON array of recommendation strings.
    recommendations TEXT,

    graded_at DATETIME NOT NULL,

    -- Store weights used for the overall score to keep historical grades reproducible.
    process_weight_permille INTEGER NOT NULL DEFAULT 400 CHECK (process_weight_permille >= 0),
    risk_weight_permille INTEGER NOT NULL DEFAULT 300 CHECK (risk_weight_permille >= 0),
    execution_weight_permille INTEGER NOT NULL DEFAULT 200 CHECK (execution_weight_permille >= 0),
    documentation_weight_permille INTEGER NOT NULL DEFAULT 100 CHECK (documentation_weight_permille >= 0),

    CHECK (process_weight_permille + risk_weight_permille + execution_weight_permille + documentation_weight_permille = 1000)
);

INSERT INTO trade_grades_new (
    id,
    created_at,
    updated_at,
    deleted_at,
    trade_id,
    overall_score,
    overall_grade,
    process_score,
    risk_score,
    execution_score,
    documentation_score,
    recommendations,
    graded_at,
    process_weight_permille,
    risk_weight_permille,
    execution_weight_permille,
    documentation_weight_permille
)
SELECT
    id,
    created_at,
    updated_at,
    deleted_at,
    trade_id,
    overall_score,
    overall_grade,
    process_score,
    risk_score,
    execution_score,
    documentation_score,
    recommendations,
    graded_at,
    400,
    300,
    200,
    100
FROM trade_grades;

DROP TABLE trade_grades;
ALTER TABLE trade_grades_new RENAME TO trade_grades;

CREATE INDEX IF NOT EXISTS idx_trade_grades_trade_id ON trade_grades(trade_id);
CREATE INDEX IF NOT EXISTS idx_trade_grades_graded_at ON trade_grades(graded_at);

PRAGMA foreign_keys=ON;
