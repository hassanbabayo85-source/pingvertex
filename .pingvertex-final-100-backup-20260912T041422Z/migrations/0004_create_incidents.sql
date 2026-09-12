CREATE TABLE IF NOT EXISTS incidents (
    id UUID PRIMARY KEY,
    monitor_id UUID NOT NULL REFERENCES monitors(id) ON DELETE CASCADE,

    started_at TIMESTAMPTZ NOT NULL,
    resolved_at TIMESTAMPTZ,

    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_incidents_monitor_id
    ON incidents (monitor_id);

CREATE INDEX IF NOT EXISTS idx_incidents_resolved_at
    ON incidents (resolved_at);

CREATE INDEX IF NOT EXISTS idx_incidents_monitor_resolved_at
    ON incidents (monitor_id, resolved_at);

CREATE INDEX IF NOT EXISTS idx_incidents_monitor_started_at
    ON incidents (monitor_id, started_at DESC);
