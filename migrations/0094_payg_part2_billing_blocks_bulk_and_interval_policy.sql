BEGIN;

-- Durable per-monitor billing block. We deliberately keep this outside the
-- Monitor aggregate so existing monitor APIs and domain constructors remain
-- backward compatible. A block means PAYG is temporarily unable to execute
-- this monitor because the tenant has insufficient available wallet credit.
CREATE TABLE IF NOT EXISTS monitor_billing_blocks (
    monitor_id UUID PRIMARY KEY REFERENCES monitors(id) ON DELETE CASCADE,
    tenant_id UUID NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    reason TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT chk_monitor_billing_block_reason CHECK (length(trim(reason)) BETWEEN 1 AND 500)
);

CREATE INDEX IF NOT EXISTS idx_monitor_billing_blocks_tenant
    ON monitor_billing_blocks(tenant_id);

-- PAYG execution-frequency policy is server controlled and versionable by
-- migration. This prevents clients from choosing unsafe intervals and keeps
-- the policy out of UI code.
CREATE TABLE IF NOT EXISTS monitor_interval_policies (
    kind TEXT PRIMARY KEY,
    min_interval_seconds INTEGER NOT NULL CHECK (min_interval_seconds >= 30),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

INSERT INTO monitor_interval_policies(kind, min_interval_seconds)
VALUES
    ('http', 30),
    ('tcp', 30),
    ('dns', 30),
    ('heartbeat', 30),
    ('flow', 60)
ON CONFLICT (kind) DO UPDATE
SET min_interval_seconds = EXCLUDED.min_interval_seconds,
    updated_at = now();

CREATE INDEX IF NOT EXISTS idx_monitor_interval_policies_min_interval
    ON monitor_interval_policies(min_interval_seconds);

COMMIT;

