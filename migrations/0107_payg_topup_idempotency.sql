-- PAYG Part 4: durable wallet top-up idempotency and replay-safe checkout data.
ALTER TABLE wallet_top_up_sessions
    ADD COLUMN IF NOT EXISTS checkout_url TEXT NOT NULL DEFAULT '';
ALTER TABLE wallet_top_up_sessions
    ADD COLUMN IF NOT EXISTS idempotency_key TEXT;
UPDATE wallet_top_up_sessions
SET idempotency_key = 'legacy-topup:' || id::text
WHERE idempotency_key IS NULL OR btrim(idempotency_key) = '';
ALTER TABLE wallet_top_up_sessions
    ALTER COLUMN idempotency_key SET NOT NULL;
CREATE UNIQUE INDEX IF NOT EXISTS uq_wallet_top_up_tenant_idempotency
    ON wallet_top_up_sessions(tenant_id, idempotency_key);

CREATE INDEX IF NOT EXISTS idx_agent_check_leases_billing_recovery
    ON agent_check_leases(completed_at)
    WHERE completed_at IS NOT NULL AND billing_reserved = TRUE;
