BEGIN;

CREATE TABLE IF NOT EXISTS wallet_settings (
    tenant_id UUID PRIMARY KEY REFERENCES tenants(id) ON DELETE CASCADE,
    low_balance_threshold_units BIGINT NOT NULL DEFAULT 5000000 CHECK (low_balance_threshold_units >= 0),
    critical_balance_threshold_units BIGINT NOT NULL DEFAULT 1000000 CHECK (critical_balance_threshold_units >= 0),
    auto_top_up_enabled BOOLEAN NOT NULL DEFAULT FALSE,
    auto_top_up_threshold_units BIGINT NOT NULL DEFAULT 5000000 CHECK (auto_top_up_threshold_units >= 0),
    auto_top_up_amount_units BIGINT NOT NULL DEFAULT 20000000 CHECK (auto_top_up_amount_units > 0),
    payment_method TEXT,
    daily_top_up_limit_units BIGINT NOT NULL DEFAULT 100000000 CHECK (daily_top_up_limit_units > 0),
    monthly_top_up_limit_units BIGINT NOT NULL DEFAULT 500000000 CHECK (monthly_top_up_limit_units > 0),
    retry_limit INTEGER NOT NULL DEFAULT 3 CHECK (retry_limit BETWEEN 0 AND 10),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT chk_wallet_threshold_order CHECK (critical_balance_threshold_units <= low_balance_threshold_units)
);

CREATE TABLE IF NOT EXISTS promotional_credits (
    id UUID PRIMARY KEY,
    tenant_id UUID NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    wallet_id UUID NOT NULL REFERENCES user_wallets(id) ON DELETE CASCADE,
    original_amount_units BIGINT NOT NULL CHECK (original_amount_units > 0),
    remaining_amount_units BIGINT NOT NULL CHECK (remaining_amount_units >= 0),
    expires_at TIMESTAMPTZ NOT NULL,
    status TEXT NOT NULL CHECK (status IN ('ACTIVE','EXPIRED')),
    idempotency_key TEXT NOT NULL UNIQUE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX IF NOT EXISTS idx_promotional_credits_expiry ON promotional_credits(status, expires_at);

CREATE TABLE IF NOT EXISTS wallet_notifications (
    id UUID PRIMARY KEY,
    tenant_id UUID NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    event_type TEXT NOT NULL,
    dedupe_key TEXT NOT NULL,
    message TEXT NOT NULL,
    read_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT uq_wallet_notification_dedupe UNIQUE (tenant_id, event_type, dedupe_key)
);
CREATE INDEX IF NOT EXISTS idx_wallet_notifications_tenant_created ON wallet_notifications(tenant_id, created_at DESC);

CREATE TABLE IF NOT EXISTS wallet_top_up_sessions (
    id UUID PRIMARY KEY,
    tenant_id UUID NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    provider TEXT NOT NULL,
    provider_checkout_id TEXT NOT NULL,
    amount_units BIGINT NOT NULL CHECK (amount_units > 0),
    currency TEXT NOT NULL,
    status TEXT NOT NULL CHECK (status IN ('PENDING','COMPLETED','FAILED')),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT uq_wallet_topup_provider_checkout UNIQUE(provider, provider_checkout_id)
);
CREATE INDEX IF NOT EXISTS idx_wallet_topup_tenant_created ON wallet_top_up_sessions(tenant_id, created_at DESC);

COMMIT;
