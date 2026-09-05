-- Wallet + PAYG core ledger.
-- Money is stored only as integer smallest currency units; never floating point.
BEGIN;

CREATE TABLE IF NOT EXISTS user_wallets (
    id UUID PRIMARY KEY,
    tenant_id UUID NOT NULL UNIQUE REFERENCES tenants(id) ON DELETE CASCADE,
    currency TEXT NOT NULL,
    balance_units BIGINT NOT NULL DEFAULT 0 CHECK (balance_units >= 0),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE IF NOT EXISTS wallet_transactions (
    id UUID PRIMARY KEY,
    wallet_id UUID NOT NULL REFERENCES user_wallets(id) ON DELETE CASCADE,
    transaction_type TEXT NOT NULL CHECK (
        transaction_type IN (
            'TOP_UP',
            'DEBIT',
            'REFUND',
            'ADJUSTMENT',
            'PROMOTIONAL_CREDIT',
            'EXPIRATION'
        )
    ),
    amount_units BIGINT NOT NULL CHECK (amount_units <> 0),
    balance_after_units BIGINT NOT NULL CHECK (balance_after_units >= 0),
    idempotency_key TEXT,
    reference_type TEXT,
    reference_id TEXT,
    description TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT uq_wallet_transaction_idempotency UNIQUE (wallet_id, idempotency_key)
);

CREATE INDEX IF NOT EXISTS idx_wallet_transactions_wallet_created
    ON wallet_transactions(wallet_id, created_at DESC);

CREATE TABLE IF NOT EXISTS credit_reservations (
    id UUID PRIMARY KEY,
    wallet_id UUID NOT NULL REFERENCES user_wallets(id) ON DELETE CASCADE,
    amount_units BIGINT NOT NULL CHECK (amount_units > 0),
    status TEXT NOT NULL CHECK (
        status IN ('PENDING', 'CAPTURED', 'RELEASED', 'EXPIRED')
    ),
    operation TEXT NOT NULL,
    request_id TEXT NOT NULL,
    expires_at TIMESTAMPTZ NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT uq_credit_reservation_request UNIQUE (wallet_id, request_id)
);

CREATE INDEX IF NOT EXISTS idx_credit_reservations_wallet_status
    ON credit_reservations(wallet_id, status);

CREATE INDEX IF NOT EXISTS idx_credit_reservations_expiry
    ON credit_reservations(status, expires_at);

CREATE TABLE IF NOT EXISTS api_usage_events (
    id UUID PRIMARY KEY,
    tenant_id UUID NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    api_key_id UUID REFERENCES api_keys(id) ON DELETE SET NULL,
    operation TEXT NOT NULL,
    units_requested BIGINT NOT NULL CHECK (units_requested >= 0),
    units_billed BIGINT NOT NULL CHECK (units_billed >= 0),
    request_id TEXT NOT NULL,
    status TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT uq_api_usage_request UNIQUE (tenant_id, request_id)
);

CREATE INDEX IF NOT EXISTS idx_api_usage_tenant_created
    ON api_usage_events(tenant_id, created_at DESC);

COMMIT;
