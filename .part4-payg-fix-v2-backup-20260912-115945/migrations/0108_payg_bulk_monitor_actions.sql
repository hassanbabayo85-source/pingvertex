CREATE TABLE IF NOT EXISTS monitor_bulk_action_jobs (
    id UUID PRIMARY KEY,
    tenant_id UUID NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    actor_user_id UUID NOT NULL REFERENCES users(id) ON DELETE RESTRICT,
    action TEXT NOT NULL CHECK (action IN ('enable','disable','pause','resume')),
    monitor_ids UUID[] NULL,
    all BOOLEAN NOT NULL DEFAULT FALSE,
    reason TEXT NULL,
    idempotency_key TEXT NULL,
    status TEXT NOT NULL DEFAULT 'PENDING' CHECK (status IN ('PENDING','RUNNING','COMPLETED','FAILED')),
    affected BIGINT NULL,
    error TEXT NULL,
    worker_id UUID NULL,
    lease_expires_at TIMESTAMPTZ NULL,
    attempts INTEGER NOT NULL DEFAULT 0,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CHECK ((all AND monitor_ids IS NULL) OR (NOT all AND monitor_ids IS NOT NULL))
);
CREATE UNIQUE INDEX IF NOT EXISTS uq_monitor_bulk_action_jobs_idem ON monitor_bulk_action_jobs(tenant_id, idempotency_key) WHERE idempotency_key IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_monitor_bulk_action_jobs_claim ON monitor_bulk_action_jobs(status, lease_expires_at, created_at);
CREATE INDEX IF NOT EXISTS idx_monitor_bulk_action_jobs_tenant ON monitor_bulk_action_jobs(tenant_id, created_at DESC);
