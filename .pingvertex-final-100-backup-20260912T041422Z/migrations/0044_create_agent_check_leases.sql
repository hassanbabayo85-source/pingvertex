CREATE TABLE IF NOT EXISTS agent_check_leases (
    check_id UUID PRIMARY KEY,
    monitor_id UUID NOT NULL REFERENCES monitors(id) ON DELETE CASCADE,
    region TEXT NOT NULL,
    agent_id TEXT NOT NULL,
    lease_expires_at TIMESTAMPTZ NOT NULL,
    claimed_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    completed_at TIMESTAMPTZ
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_agent_check_leases_active_unique
    ON agent_check_leases (monitor_id, region)
    WHERE completed_at IS NULL;

CREATE INDEX IF NOT EXISTS idx_agent_check_leases_agent_id
    ON agent_check_leases (agent_id)
    WHERE completed_at IS NULL;

CREATE INDEX IF NOT EXISTS idx_agent_check_leases_expires
    ON agent_check_leases (lease_expires_at)
    WHERE completed_at IS NULL;
