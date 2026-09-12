-- Escalation policies
CREATE TABLE escalation_policies (
    id UUID PRIMARY KEY,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    monitor_id UUID REFERENCES monitors(id) ON DELETE CASCADE,
    steps JSONB NOT NULL,
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    CONSTRAINT escalation_steps_max_5 CHECK (jsonb_array_length(steps) <= 5)
);

-- Ensure only one monitor-specific policy per monitor per user
CREATE UNIQUE INDEX idx_escalation_policies_monitor_user
    ON escalation_policies (monitor_id, user_id)
    WHERE monitor_id IS NOT NULL;

-- Ensure only one global policy per user
CREATE UNIQUE INDEX idx_escalation_policies_global_user
    ON escalation_policies (user_id)
    WHERE monitor_id IS NULL;

-- Maintenance windows
CREATE TABLE maintenance_windows (
    id UUID PRIMARY KEY,
    monitor_id UUID NOT NULL REFERENCES monitors(id) ON DELETE CASCADE,
    starts_at TIMESTAMPTZ NOT NULL,
    ends_at TIMESTAMPTZ NOT NULL,
    reason TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL,
    CONSTRAINT maintenance_window_reason_max_len CHECK (char_length(reason) <= 500),
    CONSTRAINT maintenance_window_duration_max CHECK (ends_at - starts_at <= interval '7 days'),
    CONSTRAINT maintenance_window_duration_positive CHECK (ends_at > starts_at)
);
