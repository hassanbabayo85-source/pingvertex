-- Worker 4: PAYG ledger, metering, versioned pricing, payment and invoice schema.
-- All monetary values are INTEGER/BIGINT smallest-currency units. No floating-point monetary types.
BEGIN;

-- Repair the original Worker-4 wallet ledger so the required tenant-scoped
-- idempotency invariant is explicit and database-enforced.
ALTER TABLE wallet_transactions
    ADD COLUMN IF NOT EXISTS tenant_id UUID;

UPDATE wallet_transactions wt
SET tenant_id = w.tenant_id
FROM user_wallets w
WHERE wt.wallet_id = w.id
  AND wt.tenant_id IS NULL;

UPDATE wallet_transactions
SET idempotency_key = 'legacy:' || id::text
WHERE idempotency_key IS NULL;

ALTER TABLE wallet_transactions
    ALTER COLUMN tenant_id SET NOT NULL,
    ALTER COLUMN idempotency_key SET NOT NULL;

ALTER TABLE wallet_transactions
    DROP CONSTRAINT IF EXISTS uq_wallet_transaction_idempotency;

ALTER TABLE wallet_transactions
    ADD CONSTRAINT fk_wallet_transactions_tenant
        FOREIGN KEY (tenant_id) REFERENCES tenants(id) ON DELETE CASCADE;

CREATE UNIQUE INDEX IF NOT EXISTS uq_wallet_transactions_tenant_idempotency
    ON wallet_transactions(tenant_id, idempotency_key);

CREATE INDEX IF NOT EXISTS idx_wallet_transactions_tenant_created
    ON wallet_transactions(tenant_id, created_at DESC);

-- Financial ledger rows are immutable at the database boundary too.
CREATE OR REPLACE FUNCTION prevent_wallet_transaction_mutation()
RETURNS trigger
LANGUAGE plpgsql
AS $$
BEGIN
    RAISE EXCEPTION 'wallet_transactions are immutable';
END;
$$;

DROP TRIGGER IF EXISTS trg_wallet_transactions_immutable ON wallet_transactions;
CREATE TRIGGER trg_wallet_transactions_immutable
BEFORE UPDATE OR DELETE ON wallet_transactions
FOR EACH ROW EXECUTE FUNCTION prevent_wallet_transaction_mutation();

REVOKE UPDATE, DELETE ON wallet_transactions FROM PUBLIC;
GRANT INSERT, SELECT ON wallet_transactions TO PUBLIC;

-- Versioned PAYG unit prices. Each version is immutable; new pricing creates
-- a new version rather than modifying a historical price.
CREATE TABLE IF NOT EXISTS payg_pricing_configs (
    id UUID PRIMARY KEY,
    version BIGINT NOT NULL,
    operation TEXT NOT NULL,
    unit_name TEXT NOT NULL,
    unit_price BIGINT NOT NULL CHECK (unit_price >= 0),
    currency TEXT NOT NULL,
    effective_from TIMESTAMPTZ NOT NULL,
    effective_until TIMESTAMPTZ,
    active BOOLEAN NOT NULL DEFAULT TRUE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT chk_payg_pricing_window
        CHECK (effective_until IS NULL OR effective_until > effective_from),
    CONSTRAINT uq_payg_pricing_version_operation UNIQUE (version, operation)
);

CREATE INDEX IF NOT EXISTS idx_payg_pricing_lookup
    ON payg_pricing_configs(operation, active, effective_from DESC);

CREATE OR REPLACE FUNCTION prevent_payg_pricing_mutation()
RETURNS trigger
LANGUAGE plpgsql
AS $$
BEGIN
    RAISE EXCEPTION 'payg_pricing_configs are immutable';
END;
$$;

DROP TRIGGER IF EXISTS trg_payg_pricing_immutable ON payg_pricing_configs;
CREATE TRIGGER trg_payg_pricing_immutable
BEFORE UPDATE OR DELETE ON payg_pricing_configs
FOR EACH ROW EXECUTE FUNCTION prevent_payg_pricing_mutation();

-- Usage is an immutable audit record. The existing API usage table is the
-- durable metering event store; these columns pin the exact price used.
ALTER TABLE api_usage_events
    ADD COLUMN IF NOT EXISTS unit_price BIGINT NOT NULL DEFAULT 0 CHECK (unit_price >= 0),
    ADD COLUMN IF NOT EXISTS charge_units BIGINT NOT NULL DEFAULT 0 CHECK (charge_units >= 0),
    ADD COLUMN IF NOT EXISTS currency TEXT NOT NULL DEFAULT 'USD',
    ADD COLUMN IF NOT EXISTS pricing_version BIGINT NOT NULL DEFAULT 0;

ALTER TABLE api_usage_events
    DROP CONSTRAINT IF EXISTS uq_api_usage_request;
ALTER TABLE api_usage_events
    ADD CONSTRAINT uq_api_usage_events_tenant_request UNIQUE (tenant_id, request_id);

CREATE INDEX IF NOT EXISTS idx_api_usage_tenant_created
    ON api_usage_events(tenant_id, created_at DESC);

CREATE OR REPLACE FUNCTION prevent_api_usage_event_mutation()
RETURNS trigger
LANGUAGE plpgsql
AS $$
BEGIN
    RAISE EXCEPTION 'api_usage_events are immutable';
END;
$$;

DROP TRIGGER IF EXISTS trg_api_usage_events_immutable ON api_usage_events;
CREATE TRIGGER trg_api_usage_events_immutable
BEFORE UPDATE OR DELETE ON api_usage_events
FOR EACH ROW EXECUTE FUNCTION prevent_api_usage_event_mutation();

REVOKE UPDATE, DELETE ON api_usage_events FROM PUBLIC;
GRANT INSERT, SELECT ON api_usage_events TO PUBLIC;

-- Payment schema only in Worker 4. Provider integration is Worker 5.
CREATE TABLE IF NOT EXISTS payments (
    id UUID PRIMARY KEY,
    tenant_id UUID NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    amount_units BIGINT NOT NULL CHECK (amount_units > 0),
    currency TEXT NOT NULL,
    status TEXT NOT NULL,
    provider TEXT NOT NULL,
    provider_payment_id TEXT,
    idempotency_key TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT uq_payments_tenant_idempotency UNIQUE (tenant_id, idempotency_key)
);

CREATE INDEX IF NOT EXISTS idx_payments_tenant_created
    ON payments(tenant_id, created_at DESC);

-- Invoice schema only. Worker 6 owns the invoice engine.
CREATE TABLE IF NOT EXISTS invoices (
    id UUID PRIMARY KEY,
    tenant_id UUID NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    number TEXT NOT NULL,
    status TEXT NOT NULL,
    currency TEXT NOT NULL,
    subtotal_units BIGINT NOT NULL CHECK (subtotal_units >= 0),
    total_units BIGINT NOT NULL CHECK (total_units >= 0),
    issued_at TIMESTAMPTZ,
    due_at TIMESTAMPTZ,
    paid_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT uq_invoices_tenant_number UNIQUE (tenant_id, number)
);

CREATE INDEX IF NOT EXISTS idx_invoices_tenant_created
    ON invoices(tenant_id, created_at DESC);

COMMIT;

