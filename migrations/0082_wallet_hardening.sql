BEGIN;

-- ============================================================
-- WORKER 7 HARDENING
-- Database schema required by the Worker 7 wallet implementation.
-- ============================================================

-- 1. TOP-UP SESSION HARDENING
ALTER TABLE wallet_top_up_sessions
    ADD COLUMN IF NOT EXISTS provider_payment_id TEXT;

ALTER TABLE wallet_top_up_sessions
    ADD COLUMN IF NOT EXISTS refunded_amount_units BIGINT NOT NULL DEFAULT 0;

ALTER TABLE wallet_top_up_sessions
    DROP CONSTRAINT IF EXISTS chk_wallet_topup_refunded_amount;

ALTER TABLE wallet_top_up_sessions
    ADD CONSTRAINT chk_wallet_topup_refunded_amount
    CHECK (
        refunded_amount_units >= 0
        AND refunded_amount_units <= amount_units
    );

CREATE INDEX IF NOT EXISTS idx_wallet_topup_provider_payment
    ON wallet_top_up_sessions(provider_payment_id)
    WHERE provider_payment_id IS NOT NULL;

CREATE INDEX IF NOT EXISTS idx_wallet_topup_tenant_payment
    ON wallet_top_up_sessions(tenant_id, provider_payment_id)
    WHERE provider_payment_id IS NOT NULL;


-- 2. AUTO TOP-UP ATTEMPTS
CREATE TABLE IF NOT EXISTS wallet_auto_top_up_attempts (
    id UUID PRIMARY KEY,

    tenant_id UUID NOT NULL
        REFERENCES tenants(id)
        ON DELETE CASCADE,

    amount_units BIGINT NOT NULL
        CHECK (amount_units > 0),

    status TEXT NOT NULL
        CHECK (status IN ('SUCCEEDED', 'FAILED')),

    provider_reference TEXT,
    error_message TEXT,

    attempted_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_wallet_auto_top_up_attempts_tenant_time
    ON wallet_auto_top_up_attempts(
        tenant_id,
        attempted_at DESC
    );

CREATE INDEX IF NOT EXISTS idx_wallet_auto_top_up_attempts_tenant_status_time
    ON wallet_auto_top_up_attempts(
        tenant_id,
        status,
        attempted_at DESC
    );


-- 3. AUTO TOP-UP WORKER CLAIMS
CREATE TABLE IF NOT EXISTS wallet_auto_top_up_claims (
    tenant_id UUID PRIMARY KEY
        REFERENCES tenants(id)
        ON DELETE CASCADE,

    lease_id UUID NOT NULL,

    lease_until TIMESTAMPTZ NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_wallet_auto_top_up_claims_lease_until
    ON wallet_auto_top_up_claims(lease_until);


-- 4. REFUND REQUESTS
CREATE TABLE IF NOT EXISTS wallet_refund_requests (
    id UUID PRIMARY KEY,

    tenant_id UUID NOT NULL
        REFERENCES tenants(id)
        ON DELETE CASCADE,

    provider_payment_id TEXT NOT NULL,

    amount_units BIGINT NOT NULL
        CHECK (amount_units > 0),

    idempotency_key TEXT NOT NULL,

    provider_refund_id TEXT,

    status TEXT NOT NULL
        CHECK (
            status IN (
                'PENDING',
                'SUBMITTED',
                'COMPLETED',
                'FAILED'
            )
        ),

    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),

    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),

    CONSTRAINT uq_wallet_refund_request_idempotency
        UNIQUE (tenant_id, idempotency_key)
);

CREATE UNIQUE INDEX IF NOT EXISTS uq_wallet_refund_provider_refund
    ON wallet_refund_requests(provider_refund_id)
    WHERE provider_refund_id IS NOT NULL;

CREATE INDEX IF NOT EXISTS idx_wallet_refund_requests_tenant_created
    ON wallet_refund_requests(
        tenant_id,
        created_at DESC
    );

CREATE INDEX IF NOT EXISTS idx_wallet_refund_requests_payment
    ON wallet_refund_requests(
        tenant_id,
        provider_payment_id
    );


-- 5. WALLET EMAIL NOTIFICATION STATE
ALTER TABLE wallet_notifications
    ADD COLUMN IF NOT EXISTS email_sent_at TIMESTAMPTZ;

ALTER TABLE wallet_notifications
    ADD COLUMN IF NOT EXISTS email_processing_at TIMESTAMPTZ;

CREATE INDEX IF NOT EXISTS idx_wallet_notifications_pending_email
    ON wallet_notifications(created_at ASC)
    WHERE email_sent_at IS NULL;


-- 7. TOP-UP SESSION LOOKUPS
CREATE INDEX IF NOT EXISTS idx_wallet_topup_tenant_checkout
    ON wallet_top_up_sessions(
        tenant_id,
        provider_checkout_id
    );

CREATE INDEX IF NOT EXISTS idx_wallet_topup_completed_payment
    ON wallet_top_up_sessions(
        tenant_id,
        provider_payment_id
    )
    WHERE status = 'COMPLETED'
      AND provider_payment_id IS NOT NULL;


COMMIT;
