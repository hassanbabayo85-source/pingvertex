CREATE TABLE IF NOT EXISTS diagnostic_reports (
    id UUID PRIMARY KEY,
    check_id UUID NOT NULL REFERENCES checks(id) ON DELETE CASCADE,
    steps JSONB NOT NULL,
    root_cause_summary VARCHAR(500) NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- Kowanne check ba shi da fiye da diagnostic report guda daya.
CREATE UNIQUE INDEX IF NOT EXISTS idx_diagnostic_reports_check_id
    ON diagnostic_reports (check_id);
