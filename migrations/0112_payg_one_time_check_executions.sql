-- GAP 1 — Per-region one-time executions.
--
-- A Test Now request fans out into one independent, billable execution
-- per monitor region. Each execution carries its own durable execution_id
-- (wallet idempotency component) and its own region-scoped pricing context.
BEGIN;

CREATE TABLE IF NOT EXISTS one_time_check_executions (
    id UUID PRIMARY KEY,
    request_id UUID NOT NULL REFERENCES one_time_check_requests(id) ON DELETE CASCADE,
    tenant_id UUID NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    monitor_id UUID NOT NULL REFERENCES monitors(id) ON DELETE CASCADE,
    region TEXT NOT NULL,
    execution_id UUID NOT NULL,
    status TEXT NOT NULL CHECK (status IN ('QUEUED','RUNNING','SUCCEEDED','FAILED')),
    lease_owner UUID,
    lease_expires_at TIMESTAMPTZ,
    attempted BOOLEAN NOT NULL DEFAULT FALSE,
    billable BOOLEAN NOT NULL DEFAULT FALSE,
    charge_units BIGINT NOT NULL DEFAULT 0,
    error TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    started_at TIMESTAMPTZ,
    completed_at TIMESTAMPTZ,
    CONSTRAINT uq_one_time_check_execution_id UNIQUE (execution_id),
    CONSTRAINT uq_one_time_check_execution_region UNIQUE (request_id, region)
);

CREATE INDEX IF NOT EXISTS idx_one_time_check_executions_queue
    ON one_time_check_executions (status, created_at);
CREATE INDEX IF NOT EXISTS idx_one_time_check_executions_lease
    ON one_time_check_executions (lease_expires_at)
    WHERE status = 'RUNNING';
CREATE INDEX IF NOT EXISTS idx_one_time_check_executions_request
    ON one_time_check_executions (request_id);

COMMIT;
