-- Create trade_grades table for storing trade grading evaluations
CREATE TABLE trade_grades (
    id TEXT PRIMARY KEY NOT NULL,
    created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    deleted_at TIMESTAMP,
    trade_id TEXT NOT NULL,
    overall_score INTEGER NOT NULL CHECK (overall_score >= 0 AND overall_score <= 100),
    overall_grade TEXT NOT NULL CHECK (overall_grade IN ('A', 'B', 'C', 'D', 'F')),
    process_score INTEGER NOT NULL CHECK (process_score >= 0 AND process_score <= 100),
    risk_score INTEGER NOT NULL CHECK (risk_score >= 0 AND risk_score <= 100),
    execution_score INTEGER NOT NULL CHECK (execution_score >= 0 AND execution_score <= 100),
    documentation_score INTEGER NOT NULL CHECK (documentation_score >= 0 AND documentation_score <= 100),
    recommendations TEXT, -- JSON array of recommendation strings
    graded_at TIMESTAMP NOT NULL,
    FOREIGN KEY (trade_id) REFERENCES trades(id) ON DELETE CASCADE
);