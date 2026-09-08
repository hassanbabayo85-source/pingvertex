BEGIN;
CREATE TABLE IF NOT EXISTS one_time_check_requests (
 id UUID PRIMARY KEY,
 tenant_id UUID NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
 monitor_id UUID NOT NULL REFERENCES monitors(id) ON DELETE CASCADE,
 created_by UUID NOT NULL REFERENCES users(id) ON DELETE RESTRICT,
 request_id TEXT NOT NULL,
 status TEXT NOT NULL CHECK(status IN ('QUEUED','RUNNING','SUCCEEDED','FAILED')),
 execution_id UUID NOT NULL,
 lease_owner UUID,
 lease_expires_at TIMESTAMPTZ,
 error TEXT,
 created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
 started_at TIMESTAMPTZ,
 completed_at TIMESTAMPTZ,
 CONSTRAINT uq_one_time_check_request UNIQUE(tenant_id,request_id),
 CONSTRAINT uq_one_time_check_execution UNIQUE(execution_id)
);
CREATE INDEX IF NOT EXISTS idx_one_time_check_queue ON one_time_check_requests(status,created_at);
CREATE INDEX IF NOT EXISTS idx_one_time_check_lease ON one_time_check_requests(lease_expires_at) WHERE status='RUNNING';
COMMIT;
