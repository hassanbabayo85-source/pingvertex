CREATE TABLE checks (
    id UUID PRIMARY KEY,
    monitor_id UUID NOT NULL REFERENCES monitors(id) ON DELETE CASCADE,

    status SMALLINT NOT NULL,
    http_status INTEGER,
    response_time_ms BIGINT,
    error_type TEXT,

    checked_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_checks_monitor_id
    ON checks (monitor_id);

CREATE INDEX idx_checks_checked_at
    ON checks (checked_at DESC);

CREATE INDEX idx_checks_monitor_checked_at
    ON checks (monitor_id, checked_at DESC);
