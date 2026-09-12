-- B14 — Billing Provider Abstraction & Checkout.
--
-- Persists provider-agnostic checkout sessions so `/billing/checkout`
-- and `/billing/checkout/verify` are idempotent and auditable. No
-- provider-specific columns exist here: provider identity is just a
-- machine-readable string, and the provider checkout id is an opaque
-- TEXT.
CREATE TABLE checkout_sessions (
    id UUID PRIMARY KEY,
    tenant_id UUID NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    plan_tier TEXT NOT NULL,
    status TEXT NOT NULL,
    provider TEXT NOT NULL,
    provider_checkout_id TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),

    CONSTRAINT chk_checkout_sessions_plan_tier
        CHECK (plan_tier IN ('free', 'tier_1', 'tier_5', 'tier_10', 'tier_20')),
    CONSTRAINT chk_checkout_sessions_status
        CHECK (status IN ('pending', 'completed', 'failed'))
);

-- The verification endpoint looks sessions up by internal id, but an
-- index on tenant helps list/debug queries and ensures tenant isolation.
CREATE INDEX idx_checkout_sessions_tenant_id
    ON checkout_sessions (tenant_id);

-- A completed or pending session still has a unique provider checkout
-- reference, per provider. NULLs (not yet attached) are not indexed.
CREATE UNIQUE INDEX idx_checkout_sessions_provider_checkout_id
    ON checkout_sessions (provider, provider_checkout_id)
    WHERE provider_checkout_id IS NOT NULL;
