-- 0129_subscription_billing_checkout.sql
-- Task 6: subscription checkout + lifecycle persistence.
--
-- NOTE: checkout_sessions.chk_checkout_sessions_plan_tier was already
-- repaired in 0085 to ('free','starter','pro','business','enterprise')
-- and made nullable in 0119. Requirement 2 of Task 6 is satisfied there.

-- ────────────────────────────────────────────────────────────────
-- A. Append-only billing record per verified checkout.
--    (provider, provider_reference) is the idempotency key.
-- ────────────────────────────────────────────────────────────────
CREATE TABLE IF NOT EXISTS subscription_billing_records (
    id UUID PRIMARY KEY,
    subscription_id UUID NOT NULL,
    tenant_id UUID NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    plan_tier TEXT NOT NULL,
    billing_interval TEXT NOT NULL,
    currency TEXT NOT NULL,
    amount_minor BIGINT NOT NULL,
    status TEXT NOT NULL,
    provider TEXT NOT NULL,
    provider_reference TEXT NOT NULL,
    provider_payment_id TEXT,
    provider_subscription_code TEXT,
    period_start TIMESTAMPTZ NOT NULL,
    period_end TIMESTAMPTZ NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),

    CONSTRAINT chk_sbr_interval
        CHECK (billing_interval IN ('monthly','annual')),
    CONSTRAINT chk_sbr_currency
        CHECK (currency IN ('NGN','USD')),
    CONSTRAINT chk_sbr_status
        CHECK (status IN ('pending','succeeded','failed')),
    CONSTRAINT chk_sbr_amount
        CHECK (amount_minor >= 0),
    CONSTRAINT uq_sbr_provider_reference
        UNIQUE (provider, provider_reference)
);

CREATE INDEX IF NOT EXISTS idx_sbr_tenant_created
    ON subscription_billing_records (tenant_id, created_at DESC);

CREATE INDEX IF NOT EXISTS idx_sbr_provider_subscription_code
    ON subscription_billing_records (provider, provider_subscription_code)
    WHERE provider_subscription_code IS NOT NULL;

-- ────────────────────────────────────────────────────────────────
-- B. checkout_sessions: persist the selected interval + verified
--    provider payment id. amount/currency already exist (migration 0080).
-- ────────────────────────────────────────────────────────────────
ALTER TABLE checkout_sessions
    ADD COLUMN IF NOT EXISTS billing_interval TEXT,
    ADD COLUMN IF NOT EXISTS provider_payment_id TEXT;

ALTER TABLE checkout_sessions
    DROP CONSTRAINT IF EXISTS chk_checkout_sessions_billing_interval;
ALTER TABLE checkout_sessions
    ADD CONSTRAINT chk_checkout_sessions_billing_interval
    CHECK (billing_interval IS NULL OR billing_interval IN ('monthly','annual'));
