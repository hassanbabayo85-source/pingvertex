BEGIN;

CREATE TABLE IF NOT EXISTS oauth_identities (
    provider TEXT NOT NULL,
    subject TEXT NOT NULL,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (provider, subject),
    CONSTRAINT uq_oauth_identities_user_provider UNIQUE (user_id, provider),
    CONSTRAINT chk_oauth_identities_provider CHECK (provider IN ('google', 'apple')),
    CONSTRAINT chk_oauth_identities_subject_nonempty CHECK (length(trim(subject)) > 0)
);
CREATE INDEX IF NOT EXISTS idx_oauth_identities_user_id ON oauth_identities(user_id);

-- Internal QA entitlement override. It is intentionally separate from subscriptions
-- so test access can never be mistaken for a paid billing event/subscription.
CREATE TABLE IF NOT EXISTS test_access_grants (
    tenant_id UUID PRIMARY KEY REFERENCES tenants(id) ON DELETE CASCADE,
    plan_tier TEXT NOT NULL DEFAULT 'enterprise',
    enabled BOOLEAN NOT NULL DEFAULT TRUE,
    expires_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT chk_test_access_plan_tier CHECK (plan_tier IN ('free','starter','pro','business','enterprise'))
);
CREATE INDEX IF NOT EXISTS idx_test_access_enabled_expiry
    ON test_access_grants(enabled, expires_at)
    WHERE enabled = TRUE;

COMMIT;
