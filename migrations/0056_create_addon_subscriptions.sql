-- H13 — Add-on subscriptions and independent add-on pricing.
-- Pricing fields are versioned with the existing pricing config so the
-- enterprise SLA price and SMS markup are admin-configurable and auditable.
ALTER TABLE pricing_configs
    ADD COLUMN IF NOT EXISTS sms_markup_multiplier NUMERIC(20,6) NOT NULL DEFAULT 2.00,
    ADD COLUMN IF NOT EXISTS enterprise_sla_monthly_price NUMERIC(20,6) NOT NULL DEFAULT 99.00;

CREATE TABLE IF NOT EXISTS addon_subscriptions (
    id UUID PRIMARY KEY,
    tenant_id UUID NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    addon_type TEXT NOT NULL,
    quantity BIGINT NOT NULL,
    tier TEXT,
    price NUMERIC(20,6) NOT NULL,
    currency TEXT NOT NULL,
    status TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT chk_addon_subscriptions_type CHECK (addon_type IN ('sms','enterprise','rum')),
    CONSTRAINT chk_addon_subscriptions_quantity CHECK (quantity > 0),
    CONSTRAINT chk_addon_subscriptions_price CHECK (price >= 0),
    CONSTRAINT chk_addon_subscriptions_status CHECK (status IN ('pending','active','canceled')),
    CONSTRAINT chk_addon_subscriptions_currency CHECK (length(trim(currency)) > 0)
);

CREATE INDEX IF NOT EXISTS idx_addon_subscriptions_tenant_status
    ON addon_subscriptions (tenant_id, status, created_at DESC);

CREATE INDEX IF NOT EXISTS idx_addon_subscriptions_tenant_type
    ON addon_subscriptions (tenant_id, addon_type, created_at DESC);
