-- Subscriptions map a tenant to its current plan tier (B11 — Plan /
-- Subscription / Entitlement model).
--
-- Deliberately minimal: no trial/grace-period/expiry/cancellation
-- columns here — that lifecycle belongs to a later worker (B12), which
-- will ALTER this table when it needs to. A tenant with no row here is
-- resolved as being on the Free plan by `ResolveEntitlementsUseCase`
-- (application layer), not by a default row inserted here, so no
-- backfill is needed for existing tenants.
CREATE TABLE subscriptions (
    id UUID PRIMARY KEY,
    tenant_id UUID NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    plan_tier TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),

    CONSTRAINT chk_subscriptions_plan_tier
        CHECK (plan_tier IN ('free', 'tier_1', 'tier_5', 'tier_10', 'tier_20'))
);

-- Enforces one subscription per tenant and doubles as the lookup index
-- `find_by_tenant_id` relies on.
CREATE UNIQUE INDEX idx_subscriptions_tenant_id ON subscriptions (tenant_id);
