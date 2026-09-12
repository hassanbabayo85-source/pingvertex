BEGIN;

ALTER TABLE tenants
    ADD COLUMN IF NOT EXISTS status TEXT NOT NULL DEFAULT 'active';

ALTER TABLE tenants DROP CONSTRAINT IF EXISTS tenants_status_check;
ALTER TABLE tenants
    ADD CONSTRAINT tenants_status_check CHECK (status IN ('active', 'suspended'));

CREATE INDEX IF NOT EXISTS idx_tenants_status ON tenants(status);

ALTER TABLE wallet_transactions
    ADD COLUMN IF NOT EXISTS usage_event_id UUID
    REFERENCES api_usage_events(id) ON DELETE SET NULL;

CREATE INDEX IF NOT EXISTS idx_wallet_transactions_usage_event
    ON wallet_transactions(usage_event_id)
    WHERE usage_event_id IS NOT NULL;

COMMIT;
